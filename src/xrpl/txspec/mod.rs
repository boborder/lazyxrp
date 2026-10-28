//! Declarative transaction specs: one [`TxSpec`] per XRPL transaction type, driving the generic
//! TUI composer, `-x tx <TYPE> key=value…`, validation, and the `simulate` → sign → submit path
//! (`submit::submit_generic_transaction`). Coverage bookkeeping lives in `crate::coverage`.
//!
//! Adding a type: write its spec(s) in a family file here (copy `nft.rs`), then list the family's
//! `SPECS` in [`FAMILIES`]. Nothing else (Actions, composer, CLI) changes.

use std::collections::BTreeMap;

use serde_json::Value;

pub(crate) mod channel_amm;
pub(crate) mod checks_misc;
pub(crate) mod identity;
pub(crate) mod mpt;
pub(crate) mod nft;
pub(crate) mod xchain;

/// Canonical field name → raw (user-typed) value. Empty / whitespace-only means "not set".
pub type Fields = BTreeMap<String, String>;

/// Every family's specs; the only place a spec'd transaction type is registered.
pub static FAMILIES: &[&[TxSpec]] = &[
    nft::SPECS,
    xchain::SPECS,
    checks_misc::SPECS,
    channel_amm::SPECS,
    mpt::SPECS,
    identity::SPECS,
];

/// All spec'd transaction types, in registry order.
pub fn registry() -> impl Iterator<Item = &'static TxSpec> {
    FAMILIES.iter().flat_map(|specs| specs.iter())
}

fn normalized(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Case-insensitive lookup; `-` / `_` / spaces ignored (`NFTokenMint`, `nftoken-mint`, `nftoken_mint`).
pub fn lookup(name: &str) -> Option<&'static TxSpec> {
    let want = normalized(name);
    registry().find(|s| normalized(s.tx_type) == want)
}

/// How a field value is validated (and, for [`FieldKind::Time`], resolved).
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    /// Decimal integer `0..=max`.
    Uint(u32),
    /// Plain UTF-8 text of at most `max` bytes (ledger blob fields; the spec hex-encodes it).
    Text(usize),
    /// Classic address (checksum verified).
    Address,
    /// Exactly `len` hex characters (ledger object ids).
    Hex(usize),
    /// 1..=`max` distinct ids of `len` hex characters, separated by commas / spaces.
    HexList { len: usize, max: usize },
    /// `XRP:drops` or `CUR:issuer:value` (same parser as OfferCreate).
    Amount,
    /// Relative duration (`90s` / `30m` / `2h` / `7d`) or absolute Ripple-epoch seconds; resolved
    /// once to absolute seconds before simulate so simulate and the signed tx agree.
    Time,
    /// Comma-separated flag names (case-insensitive, optional `tf` prefix) → bits.
    Flags(&'static [(&'static str, u32)]),
    /// Family-specific grammar (e.g. the flat `XChainBridge` string): validator on the trimmed value.
    Custom(fn(&str) -> Result<(), String>),
}

#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    pub name: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub help: &'static str,
    /// Composer prefill (empty = blank).
    pub default: &'static str,
}

/// Typed confirmation for irreversible actions (TUI: type `phrase`; CLI: mainnet prompt / `--yes`).
#[derive(Debug, Clone, Copy)]
pub struct Confirm {
    pub phrase: &'static str,
    pub hint: &'static str,
}

pub struct TxSpec {
    pub tx_type: &'static str,
    pub family: &'static str,
    pub summary: &'static str,
    pub fields: &'static [FieldSpec],
    pub confirm: Option<Confirm>,
    /// Cross-field rules, run after every field passed its own validation. Values are `prepare`d
    /// (trimmed, empty dropped, times absolute).
    pub check: fn(&Fields) -> Result<(), String>,
    /// Unsigned `tx_json` for `simulate` (`Sequence` set, no `Fee` / `LastLedgerSequence`).
    pub build_json: fn(account: &str, &Fields, sequence: u32) -> color_eyre::Result<Value>,
    /// Sign with the typed `xrpl::models` struct → tx blob hex.
    pub sign: fn(
        &xrpl::wallet::Wallet,
        account: &str,
        &Fields,
        sequence: u32,
        fee_drops: u32,
        last_ledger_sequence: u32,
    ) -> color_eyre::Result<String>,
}

impl std::fmt::Debug for TxSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSpec")
            .field("tx_type", &self.tx_type)
            .finish_non_exhaustive()
    }
}

/// Trimmed value, `None` when unset / blank.
pub(crate) fn val<'a>(fields: &'a Fields, name: &str) -> Option<&'a str> {
    fields.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// Largest legal XRP amount in drops (100 billion XRP).
pub(crate) const MAX_DROPS: u64 = 100_000_000_000_000_000;

/// The one strict XRP-drops grammar: decimal digits only (no sign, no whitespace), no leading
/// zeros except `0` itself, `<=` [`MAX_DROPS`]. Zero is allowed here; callers that need a
/// positive amount check it. The canonical `n.to_string()` is what goes on the wire.
pub(crate) fn parse_drops(v: &str) -> Result<u64, String> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected XRP drops (an integer of digits only)".into());
    }
    if v.len() > 1 && v.starts_with('0') {
        return Err("XRP drops must not have leading zeros".into());
    }
    match v.parse::<u64>() {
        Ok(n) if n <= MAX_DROPS => Ok(n),
        _ => Err(format!("drops must be 0..={MAX_DROPS}")),
    }
}

/// OR of the flag bits named in `names` (validated by [`FieldKind::Flags`]).
pub(crate) fn flag_bits(table: &[(&str, u32)], names: &str) -> u32 {
    split_list(names)
        .filter_map(|n| flag_lookup(table, n))
        .fold(0, |acc, bit| acc | bit)
}

fn flag_lookup(table: &[(&str, u32)], name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    let n = n.strip_prefix("tf").unwrap_or(&n);
    table
        .iter()
        .find(|(flag, _)| flag.eq_ignore_ascii_case(n))
        .map(|(_, bit)| *bit)
}

fn split_list(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| c == ',' || c == '+' || c.is_whitespace())
        .filter(|p| !p.is_empty())
}

/// Ids of a [`FieldKind::HexList`] value, upper-cased.
pub(crate) fn id_list(value: &str) -> Vec<String> {
    split_list(value).map(str::to_ascii_uppercase).collect()
}

fn is_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl FieldKind {
    /// Validate one non-empty value.
    pub fn validate(&self, v: &str) -> Result<(), String> {
        match *self {
            Self::Uint(max) => {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(format!("expected an integer 0..={max}"));
                }
                match v.parse::<u32>() {
                    Ok(n) if n <= max => Ok(()),
                    _ => Err(format!("must be at most {max}")),
                }
            }
            Self::Text(max) => {
                if v.len() > max {
                    Err(format!("{} bytes, at most {max}", v.len()))
                } else {
                    Ok(())
                }
            }
            Self::Address => {
                if xrpl::core::addresscodec::is_valid_classic_address(v) {
                    Ok(())
                } else {
                    Err("expected a valid classic address (r…)".into())
                }
            }
            Self::Hex(len) => {
                if v.len() == len && is_hex(v) {
                    Ok(())
                } else {
                    Err(format!("expected {len} hex characters (got {})", v.len()))
                }
            }
            Self::HexList { len, max } => {
                let ids: Vec<&str> = split_list(v).collect();
                if ids.is_empty() || ids.len() > max {
                    return Err(format!("expected 1..={max} ids"));
                }
                for id in &ids {
                    Self::Hex(len).validate(id)?;
                }
                let mut seen: Vec<String> = ids.iter().map(|i| i.to_ascii_uppercase()).collect();
                seen.sort_unstable();
                if seen.windows(2).any(|w| w[0] == w[1]) {
                    return Err("ids must be unique".into());
                }
                Ok(())
            }
            Self::Amount => crate::signing::parse_offer_amount(v)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::Time => {
                if v.bytes().all(|b| b.is_ascii_digit()) {
                    v.parse::<u32>()
                        .map(|_| ())
                        .map_err(|_| "Ripple-epoch seconds must fit u32".into())
                } else {
                    crate::signing::parse_relative_duration(v)
                        .map(|_| ())
                        .map_err(|_| "use 90s / 30m / 2h / 7d or Ripple-epoch seconds".into())
                }
            }
            Self::Flags(table) => {
                for name in split_list(v) {
                    if flag_lookup(table, name).is_none() {
                        let valid: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
                        return Err(format!(
                            "unknown flag {name:?} (valid: {})",
                            valid.join(", ")
                        ));
                    }
                }
                Ok(())
            }
            Self::Custom(check) => check(v),
        }
    }
}

impl FieldSpec {
    /// Validate one raw form value (blank is fine unless required).
    pub fn validate_value(&self, raw: &str) -> Result<(), String> {
        match raw.trim() {
            "" if self.required => Err("required".into()),
            "" => Ok(()),
            v => self.kind.validate(v),
        }
    }
}

impl TxSpec {
    /// Field-by-field check plus the cross-field rules. Errors name the field.
    pub fn validate(&self, fields: &Fields) -> Result<(), String> {
        let prepared = self.trimmed(fields)?;
        for f in self.fields {
            f.validate_value(prepared.get(f.name).map_or("", String::as_str))
                .map_err(|e| format!("{}: {e}", f.name))?;
        }
        (self.check)(&prepared)
    }

    /// Validate, then return the fields as signed: trimmed, blanks dropped, `Time` fields turned
    /// into absolute Ripple-epoch seconds (`ripple_now` + duration). Call once per submit.
    pub fn prepare(&self, fields: &Fields, ripple_now: u64) -> Result<Fields, String> {
        self.validate(fields)?;
        let mut out = self.trimmed(fields)?;
        for f in self
            .fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::Time))
        {
            let Some(v) = out.get_mut(f.name) else {
                continue;
            };
            if v.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let secs = crate::signing::parse_relative_duration(v).map_err(|e| e.to_string())?;
            let abs = u32::try_from(ripple_now + u64::from(secs))
                .map_err(|_| format!("{}: time does not fit u32", f.name))?;
            *v = abs.to_string();
        }
        Ok(out)
    }

    /// Trimmed, blanks dropped; unknown field names are an error.
    fn trimmed(&self, fields: &Fields) -> Result<Fields, String> {
        let mut out = Fields::new();
        for (k, v) in fields {
            if !self.fields.iter().any(|f| f.name == k) {
                return Err(format!(
                    "unknown field {k} for {} (fields: {})",
                    self.tx_type,
                    self.field_names()
                ));
            }
            if !v.trim().is_empty() {
                out.insert(k.clone(), v.trim().to_string());
            }
        }
        Ok(out)
    }

    pub fn field_names(&self) -> String {
        self.fields
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `key=value` CLI arguments → canonical [`Fields`] (names case-insensitive, no duplicates).
    pub fn parse_cli_fields<S: AsRef<str>>(&self, raw: &[S]) -> Result<Fields, String> {
        let mut out = Fields::new();
        for arg in raw {
            let arg = arg.as_ref();
            let (k, v) = arg
                .split_once('=')
                .ok_or_else(|| format!("expected Field=value, got {arg:?}"))?;
            let spec = self
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(k.trim()))
                .ok_or_else(|| {
                    format!(
                        "unknown field {k:?} for {} (fields: {})",
                        self.tx_type,
                        self.field_names()
                    )
                })?;
            if out.insert(spec.name.to_string(), v.to_string()).is_some() {
                return Err(format!("duplicate field {}", spec.name));
            }
        }
        Ok(out)
    }

    /// One-line description of a prepared form (CLI prompt, composer preview).
    pub fn summary_line(&self, fields: &Fields) -> String {
        let parts: Vec<String> = self
            .fields
            .iter()
            .filter_map(|f| val(fields, f.name).map(|v| format!("{}={v}", f.name)))
            .collect();
        format!("{} {}", self.tx_type, parts.join(" "))
    }
}

/// `CommonFields` for a spec'd tx; with `fee_lls` it is the signed form (`Fee` +
/// `LastLedgerSequence` from simulate), without it the unsigned form for `simulate`.
macro_rules! common_fields {
    ($account:expr, $ty:ident, $sequence:expr, $fee_lls:expr) => {{
        let mut common =
            xrpl::models::transactions::CommonFields::from_account($account.to_string())
                .with_transaction_type(xrpl::models::transactions::TransactionType::$ty)
                .with_sequence($sequence);
        if let Some((fee, lls)) = $fee_lls {
            common = common
                .with_fee(xrpl::models::XRPAmount::from(fee.to_string()))
                .with_last_ledger_sequence(lls);
        }
        common
    }};
}
pub(crate) use common_fields;

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// TC-249: registry integrity — unique type names, unique field names per type, defaults
    /// valid for their own kind, a confirm phrase only on irreversible types, lookup tolerant of
    /// case and `-` / `_`.
    #[test]
    fn registry_is_well_formed() {
        let specs: Vec<&TxSpec> = registry().collect();
        assert!(specs.len() >= 5);
        let mut names: Vec<&str> = specs.iter().map(|s| s.tx_type).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), specs.len(), "duplicate tx_type");
        for s in &specs {
            let mut fs: Vec<&str> = s.fields.iter().map(|f| f.name).collect();
            fs.sort_unstable();
            fs.dedup();
            assert_eq!(fs.len(), s.fields.len(), "{}: duplicate field", s.tx_type);
            for f in s.fields {
                assert!(
                    f.default.is_empty() || f.kind.validate(f.default).is_ok(),
                    "{}.{}: default {:?} fails its own kind",
                    s.tx_type,
                    f.name,
                    f.default
                );
                assert!(!f.help.is_empty(), "{}.{}: help text", s.tx_type, f.name);
            }
            assert_eq!(lookup(s.tx_type).map(|x| x.tx_type), Some(s.tx_type));
        }
        for alias in ["nftoken-mint", "NFTOKEN_MINT", " nftokenmint"] {
            assert_eq!(
                lookup(alias).map(|s| s.tx_type),
                Some("NFTokenMint"),
                "{alias}"
            );
        }
        assert!(
            lookup("Payment").is_none(),
            "hand-written types are not specs"
        );
        let phrase_of = |t: &str| {
            specs
                .iter()
                .find(|s| s.tx_type == t)
                .and_then(|s| s.confirm)
        };
        assert_eq!(phrase_of("NFTokenBurn").map(|c| c.phrase), Some("BURN"));
        assert_eq!(
            phrase_of("NFTokenAcceptOffer").map(|c| c.phrase),
            Some("ACCEPT")
        );
        assert!(phrase_of("NFTokenMint").is_none(), "mint is reversible");
        for c in specs.iter().filter_map(|s| s.confirm) {
            assert!(
                !c.phrase.is_empty() && c.phrase.bytes().all(|b| b.is_ascii_uppercase()),
                "confirm phrase {:?} must be an uppercase word",
                c.phrase
            );
        }
    }

    /// TC-249: `key=value` CLI arguments → canonical names (case-insensitive); malformed, unknown
    /// and duplicate fields are errors.
    #[test]
    fn cli_fields_parse() {
        let mint = lookup("NFTokenMint").unwrap();
        assert_eq!(
            mint.parse_cli_fields(&["nftokentaxon=5", "URI=a=b"])
                .unwrap(),
            fields(&[("NFTokenTaxon", "5"), ("URI", "a=b")])
        );
        assert!(
            mint.parse_cli_fields(&["NFTokenTaxon"])
                .unwrap_err()
                .contains("Field=value")
        );
        assert!(
            mint.parse_cli_fields(&["Taxon=1"])
                .unwrap_err()
                .contains("unknown field")
        );
        assert!(
            mint.parse_cli_fields(&["URI=a", "uri=b"])
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(mint.parse_cli_fields::<&str>(&[]).unwrap().is_empty());
    }

    /// TC-249: `prepare` trims, drops blanks and fixes `Time` fields once: a relative duration
    /// becomes `now + secs`, an absolute value is kept, overflow is refused.
    #[test]
    fn prepare_resolves_times_once() {
        let offer = lookup("NFTokenCreateOffer").unwrap();
        let base = [
            (
                "NFTokenID",
                "000B013A95F14B0044F78A264E41713C64B5F89242540EE208C3098E00000D65",
            ),
            ("Amount", " XRP:5 "),
            ("Flags", "sell"),
            ("Owner", "  "),
        ];
        let mut f = fields(&base);
        f.insert("Expiration".into(), "2h".into());
        let p = offer.prepare(&f, 1_000).unwrap();
        assert_eq!(p["Expiration"], "8200");
        assert_eq!(p["Amount"], "XRP:5");
        assert!(!p.contains_key("Owner"), "blank dropped");
        f.insert("Expiration".into(), "123456".into());
        assert_eq!(offer.prepare(&f, 1_000).unwrap()["Expiration"], "123456");
        f.insert("Expiration".into(), "7d".into());
        assert!(
            offer
                .prepare(&f, u64::from(u32::MAX))
                .unwrap_err()
                .contains("u32")
        );
        assert!(
            offer
                .prepare(&fields(&[("Bogus", "1")]), 0)
                .unwrap_err()
                .contains("unknown field")
        );
    }
}

#[cfg(test)]
mod drops_tests {
    use super::*;

    /// TC-357: parse_drops is strict
    /// One strict drops grammar for every XRP amount: digits only, canonical, `<= 1e17`.
    #[test]
    fn parse_drops_is_strict() {
        assert_eq!(parse_drops("0"), Ok(0));
        assert_eq!(parse_drops("100"), Ok(100));
        assert_eq!(parse_drops(&MAX_DROPS.to_string()), Ok(MAX_DROPS));
        for bad in ["", "+5", "-5", "007", "00", " 5", "5 ", "1.5", "1e3"] {
            assert!(parse_drops(bad).is_err(), "{bad:?}");
        }
        assert!(parse_drops(&(MAX_DROPS + 1).to_string()).is_err());
        assert!(parse_drops("18446744073709551616").is_err());
        for bad in ["XRP:+5", "XRP:007", "XRP:0", "XRP:100000000000000001"] {
            assert!(FieldKind::Amount.validate(bad).is_err(), "{bad}");
        }
        assert!(FieldKind::Amount.validate("XRP:5").is_ok());
    }
}
