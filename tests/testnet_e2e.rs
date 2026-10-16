//! Opt-in live testnet E2E for the `-x` write paths (public faucet + node; never in CI).
//! Run: `mise run e2e-testnet` (= `cargo test --test testnet_e2e -- --ignored --nocapture`).
//! Mocks once diverged from live `simulate` (TC-200); this run is the release-time check.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run `lazyxrp --network $LAZYXRP_E2E_NETWORK (default testnet) -x <args>` with isolated config
/// (`devnet` is a quiet fallback while the testnet open-ledger fee is escalated); `seed` signs.
/// Panics on failure.
fn lazyxrp(root: &TempRoot, seed: Option<&str>, args: &[&str]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lazyxrp"));
    command
        .env("LAZYXRP_CONFIG", root.0.join("config"))
        .env("LAZYXRP_DATA", root.0.join("data"))
        .env_remove("XRPL_MNEMONIC")
        .env_remove("XRPL_RPC_SERVER")
        .env_remove("XRPL_WS_SERVER")
        .env_remove("XRPL_NETWORK")
        .args([
            "--network",
            &std::env::var("LAZYXRP_E2E_NETWORK").unwrap_or_else(|_| "testnet".into()),
            "-x",
        ])
        .args(args);
    match seed {
        Some(seed) => command.env("XRPL_SEED", seed),
        None => command.env_remove("XRPL_SEED"),
    };
    let output = command.output().expect("spawn lazyxrp");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    println!("$ lazyxrp -x {}\n{stdout}", args.join(" "));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let hint = if stderr.contains("exceeds the requested tx limit") {
        "\nhint: the network's open-ledger fee is escalated (check the `fee` RPC); retry with LAZYXRP_E2E_NETWORK=devnet"
    } else {
        ""
    };
    assert!(
        output.status.success(),
        "lazyxrp -x {} failed: {stderr}{hint}",
        args.join(" ")
    );
    stdout
}

fn field<'a>(stdout: &'a str, prefix: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing {prefix:?} in:\n{stdout}"))
        .trim()
}

/// Faucet-created accounts appear once the funding payment validates.
fn wait_activated(root: &TempRoot, address: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !lazyxrp(root, None, &["account-status", address]).contains("Status: Activated") {
        assert!(Instant::now() < deadline, "{address} not activated in 60 s");
        std::thread::sleep(Duration::from_secs(3));
    }
}

#[test]
#[ignore = "live testnet: public faucet + node; run via `mise run e2e-testnet`"]
fn testnet_write_paths_validate() {
    let dir = std::env::temp_dir().join(format!("lazyxrp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("config")).expect("config dir");
    std::fs::create_dir_all(dir.join("data")).expect("data dir");
    let root = TempRoot(dir);

    let funded = lazyxrp(&root, None, &["faucet"]);
    let seed = field(&funded, "Seed:").to_owned();
    let account = field(&funded, "Funded")
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let issuer_out = lazyxrp(&root, None, &["faucet"]);
    let issuer = field(&issuer_out, "Funded")
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    wait_activated(&root, &account);
    wait_activated(&root, &issuer);

    let sent = lazyxrp(&root, Some(&seed), &["send", &issuer, "--amount", "1"]);
    assert_eq!(field(&sent, "Validated:"), "tesSUCCESS");

    let pays = format!("USD:{issuer}:1");
    let created = lazyxrp(&root, Some(&seed), &["offer-create", "XRP:1000000", &pays]);
    assert_eq!(field(&created, "Validated:"), "tesSUCCESS");
    let offer_seq = field(&created, "OfferCreate from")
        .rsplit("offer seq ")
        .next()
        .and_then(|s| s.trim_end_matches(')').parse::<u32>().ok())
        .expect("offer seq in OfferCreate line");

    let offers = lazyxrp(&root, None, &["offers", &account]);
    assert!(
        offers.contains(&format!("\"seq\": {offer_seq}")),
        "{offers}"
    );

    let cancelled = lazyxrp(
        &root,
        Some(&seed),
        &["offer-cancel", &offer_seq.to_string()],
    );
    assert_eq!(field(&cancelled, "Validated:"), "tesSUCCESS");
    let offers = lazyxrp(&root, None, &["offers", &account]);
    assert_eq!(offers.trim(), "[]");
}

/// `-x nfts <account>` rows (`nft_id`, `taxon`, …) as JSON.
fn account_nfts(root: &TempRoot, account: &str) -> Vec<serde_json::Value> {
    let out = lazyxrp(root, None, &["nfts", account]);
    serde_json::from_str(out.trim()).expect("nfts JSON array")
}

/// Poll `nfts` until `want(rows)` holds (validated ledgers lag the submit result).
fn wait_nfts(
    root: &TempRoot,
    account: &str,
    what: &str,
    want: impl Fn(&[serde_json::Value]) -> bool,
) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let rows = account_nfts(root, account);
        if want(&rows) {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: not seen in 60 s: {rows:?}"
        );
        std::thread::sleep(Duration::from_secs(3));
    }
}

#[test]
#[ignore = "live testnet: public faucet + node; run via `mise run e2e-testnet`"]
fn testnet_nft_mint_burn_validate() {
    const TAXON: u64 = 4242;
    let dir = std::env::temp_dir().join(format!("lazyxrp-e2e-nft-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("config")).expect("config dir");
    std::fs::create_dir_all(dir.join("data")).expect("data dir");
    let root = TempRoot(dir);

    let funded = lazyxrp(&root, None, &["faucet"]);
    let seed = field(&funded, "Seed:").to_owned();
    let account = field(&funded, "Funded")
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    wait_activated(&root, &account);
    assert!(account_nfts(&root, &account).is_empty());

    let minted = lazyxrp(
        &root,
        Some(&seed),
        &[
            "nft-mint",
            "--uri",
            "https://example.com/lazyxrp-e2e.json",
            "--taxon",
            &TAXON.to_string(),
            "--flags",
            "burnable,transferable",
            "--transfer-fee",
            "500",
        ],
    );
    assert_eq!(field(&minted, "Validated:"), "tesSUCCESS");
    println!("mint hash: {}", field(&minted, "Transaction Hash:"));
    let rows = wait_nfts(&root, &account, "minted NFT", |rows| {
        rows.iter().any(|r| r["taxon"] == TAXON)
    });
    let nft_id = rows
        .iter()
        .find(|r| r["taxon"] == TAXON)
        .and_then(|r| r["nft_id"].as_str())
        .expect("nft_id")
        .to_owned();
    assert_eq!(nft_id.len(), 64);

    let burned = lazyxrp(&root, Some(&seed), &["nft-burn", &nft_id]);
    assert_eq!(field(&burned, "Validated:"), "tesSUCCESS");
    println!("burn hash: {}", field(&burned, "Transaction Hash:"));
    wait_nfts(&root, &account, "NFT gone after burn", |rows| {
        rows.is_empty()
    });
}
