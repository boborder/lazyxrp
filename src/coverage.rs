//! XRPL transaction coverage — the single source of truth for "which transaction types does
//! lazyxrp read / write". The ROADMAP.md "XRPL coverage" table mirrors [`markdown_table`] (a drift
//! test fails when they disagree); `lazyxrp -x coverage` prints the table to paste.
//!
//! - `read`: a TX-detail parser is registered (`components::shared::tx_detail`).
//! - `Spec` rows come from the `xrpl::txspec` registry (add a type there; it appears here).
//! - `Existing` rows are the hand-written per-type write paths (clean cutover to specs is later work).
//! - `Planned` = xrpl-rust has a typed model, not wired yet; `Unsupported` = no typed model in the
//!   locked xrpl-rust (1.3.0), so it cannot be signed through the typed path.

use crate::components::shared::tx_detail::registered_tx_types;
use crate::xrpl::txspec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Existing,
    Spec,
    Planned,
    Unsupported,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Existing => "Existing",
            Self::Spec => "Spec",
            Self::Planned => "Planned",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub tx_type: &'static str,
    pub read: bool,
    pub write_tui: bool,
    pub write_cli: bool,
    pub status: Status,
    pub note: &'static str,
}

/// `(type, TUI, CLI)` — hand-written write paths.
const EXISTING: &[(&str, bool, bool)] = &[
    ("Payment", true, true),
    ("AccountSet", true, false),
    ("TrustSet", true, false),
    ("OfferCreate", true, true),
    ("OfferCancel", true, true),
    ("SetRegularKey", true, false),
    ("EscrowCreate", true, false),
    ("EscrowFinish", true, false),
    ("EscrowCancel", true, false),
    ("AMMDeposit", true, true),
    ("AMMWithdraw", true, true),
    ("AccountDelete", true, true),
];

/// `(type, note)` — typed model exists in xrpl-rust 1.3.0, not wired yet.
const PLANNED: &[(&str, &str)] = &[
    (
        "VaultCreate",
        "Single-asset vault: Asset may be an MPT, which the flat Amount grammar cannot express",
    ),
    (
        "VaultSet",
        "Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets)",
    ),
    (
        "VaultDelete",
        "Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets)",
    ),
    (
        "VaultDeposit",
        "Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express",
    ),
    (
        "VaultWithdraw",
        "Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express",
    ),
    (
        "VaultClawback",
        "Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express",
    ),
];

/// `(type, reason)` — no typed model in the locked xrpl-rust.
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "NFTokenModify",
        "no typed model in xrpl-rust 1.3.0 (dynamic NFT)",
    ),
    ("AMMClawback", "no typed model in xrpl-rust 1.3.0"),
    ("Batch", "no typed model in xrpl-rust 1.3.0"),
    ("DelegateSet", "no typed model in xrpl-rust 1.3.0"),
    ("LedgerStateFix", "no typed model in xrpl-rust 1.3.0"),
    (
        "XChainAddClaimAttestation",
        "witness-server signed attestation; cannot be authored in a form",
    ),
    (
        "XChainAddAccountCreateAttestation",
        "witness-server signed attestation; cannot be authored in a form",
    ),
    (
        "ConfidentialMPTConvert",
        "needs zero-knowledge proofs; no typed model in xrpl-rust",
    ),
    (
        "ConfidentialMPTConvertBack",
        "needs zero-knowledge proofs; no typed model in xrpl-rust",
    ),
    (
        "ConfidentialMPTSend",
        "needs zero-knowledge proofs; no typed model in xrpl-rust",
    ),
    (
        "ConfidentialMPTMergeInbox",
        "needs zero-knowledge proofs; no typed model in xrpl-rust",
    ),
    (
        "ConfidentialMPTClawback",
        "needs zero-knowledge proofs; no typed model in xrpl-rust",
    ),
];

/// Every XRPL transaction type the app knows, grouped Existing → Spec → Planned → Unsupported.
pub(crate) fn rows() -> Vec<Row> {
    let read: Vec<&str> = registered_tx_types().collect();
    let has_read = |t: &str| read.contains(&t);
    let mut out = Vec::new();
    for &(tx_type, tui, cli) in EXISTING {
        out.push(Row {
            tx_type,
            read: has_read(tx_type),
            write_tui: tui,
            write_cli: cli,
            status: Status::Existing,
            note: "",
        });
    }
    for spec in txspec::registry() {
        out.push(Row {
            tx_type: spec.tx_type,
            read: has_read(spec.tx_type),
            write_tui: true,
            write_cli: true,
            status: Status::Spec,
            note: spec.family,
        });
    }
    for (group, status) in [
        (PLANNED, Status::Planned),
        (UNSUPPORTED, Status::Unsupported),
    ] {
        for &(tx_type, note) in group {
            out.push(Row {
                tx_type,
                read: has_read(tx_type),
                write_tui: false,
                write_cli: false,
                status,
                note,
            });
        }
    }
    out
}

/// `(writable, total)` — writable = TUI or CLI write path.
pub(crate) fn write_counts(rows: &[Row]) -> (usize, usize) {
    (
        rows.iter().filter(|r| r.write_tui || r.write_cli).count(),
        rows.len(),
    )
}

fn mark(on: bool) -> &'static str {
    if on { "✓" } else { "–" }
}

/// The ROADMAP.md "XRPL coverage" table (header + one row per type).
pub(crate) fn markdown_table() -> String {
    let mut out = String::from(
        "| Type | Read | TUI write | CLI write | Status | Note |\n|------|------|-----------|-----------|--------|------|\n",
    );
    for r in rows() {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            r.tx_type,
            mark(r.read),
            mark(r.write_tui),
            mark(r.write_cli),
            r.status.label(),
            r.note
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = "<!-- xrpl-coverage:start -->";
    const END: &str = "<!-- xrpl-coverage:end -->";

    fn between<'a>(text: &'a str, name: &str) -> &'a str {
        let from = text
            .find(START)
            .unwrap_or_else(|| panic!("{name}: missing {START}"))
            + START.len();
        let to = text
            .find(END)
            .unwrap_or_else(|| panic!("{name}: missing {END}"));
        &text[from..to]
    }

    /// Data rows of a markdown table as trimmed cells, skipping header and separator.
    fn parse_rows(table: &str) -> Vec<Vec<String>> {
        table
            .lines()
            .filter(|l| l.trim_start().starts_with('|'))
            .skip(2)
            .map(|l| {
                l.trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_string())
                    .collect()
            })
            .collect()
    }

    /// TC-248: the ROADMAP.md "XRPL coverage" table and the registry agree row for row
    /// (type, read, TUI, CLI, status; the Note column is free text), and the README count matches.
    #[test]
    fn roadmap_table_matches_registry() {
        let roadmap = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/ROADMAP.md"))
            .expect("ROADMAP.md");
        let want = parse_rows(&markdown_table());
        let got = parse_rows(between(&roadmap, "ROADMAP.md"));
        let key = |r: &Vec<String>| r[..5].to_vec();
        assert_eq!(
            got.iter().map(key).collect::<Vec<_>>(),
            want.iter().map(key).collect::<Vec<_>>(),
            "ROADMAP.md XRPL coverage table drifted from src/coverage.rs; regenerate with `lazyxrp -x coverage`:\n{}",
            markdown_table()
        );

        let (writable, total) = write_counts(&rows());
        let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
            .expect("README.md");
        assert!(
            readme.contains(&format!("**{writable} / {total}**")),
            "README.md feature list must say **{writable} / {total}** XRPL transaction types writable"
        );
    }

    /// TC-248: the registry is consistent — no type listed twice, every Spec row is writable from
    /// TUI + CLI and readable, and no read parser is registered for an unknown type.
    #[test]
    fn registry_is_consistent() {
        let rows = rows();
        let mut names: Vec<&str> = rows.iter().map(|r| r.tx_type).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "a transaction type is listed twice");
        for r in rows.iter().filter(|r| r.status == Status::Spec) {
            assert!(
                r.write_tui && r.write_cli && r.read,
                "{} needs read + TUI + CLI",
                r.tx_type
            );
        }
        for r in rows
            .iter()
            .filter(|r| matches!(r.status, Status::Planned | Status::Unsupported))
        {
            assert!(
                !r.write_tui && !r.write_cli,
                "{} is not writable",
                r.tx_type
            );
        }
        for t in registered_tx_types() {
            assert!(names.contains(&t), "read parser for unlisted type {t}");
        }
    }
}
