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

/// Run `lazyxrp --network testnet -x <args>` with isolated config; `seed` signs. Panics on failure.
fn lazyxrp(root: &TempRoot, seed: Option<&str>, args: &[&str]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lazyxrp"));
    command
        .env("LAZYXRP_CONFIG", root.0.join("config"))
        .env("LAZYXRP_DATA", root.0.join("data"))
        .env_remove("XRPL_MNEMONIC")
        .env_remove("XRPL_RPC_SERVER")
        .env_remove("XRPL_WS_SERVER")
        .env_remove("XRPL_NETWORK")
        .args(["--network", "testnet", "-x"])
        .args(args);
    match seed {
        Some(seed) => command.env("XRPL_SEED", seed),
        None => command.env_remove("XRPL_SEED"),
    };
    let output = command.output().expect("spawn lazyxrp");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    println!("$ lazyxrp -x {}\n{stdout}", args.join(" "));
    assert!(
        output.status.success(),
        "lazyxrp -x {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
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
