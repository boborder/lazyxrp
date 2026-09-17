use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread::{self, JoinHandle};

const ACCOUNT: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";

struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spawn_rpc(result: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local RPC");
    let address = listener.local_addr().expect("local RPC address");
    let result = result.to_owned();
    let handle = thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("nonblocking RPC listener");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "RPC client never connected within 30s"
                    );
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => panic!("accept RPC request: {e}"),
            }
        };
        // Accepted streams inherit the listener's nonblocking mode; restore
        // blocking I/O so read/write wait for the client like the original code.
        stream.set_nonblocking(false).expect("blocking RPC stream");
        let mut request = [0_u8; 8192];
        let _ = stream.read(&mut request).expect("read RPC request");
        let body = format!("{{\"result\":{result}}}");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write RPC response");
    });
    (format!("http://{address}"), handle)
}

fn isolated_env(name: &str) -> (TempRoot, PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("lazyxrp-cli-smoke-{name}-{}", std::process::id()));
    let config = root.join("config");
    let data = root.join("data");
    std::fs::create_dir_all(&config).expect("create config dir");
    std::fs::create_dir_all(&data).expect("create data dir");
    (TempRoot(root), config, data)
}

fn command_with_isolated_env(program: &str, name: &str) -> (Command, TempRoot) {
    let (root, config, data) = isolated_env(name);
    let mut command = Command::new(program);
    command
        .env("LAZYXRP_CONFIG", config)
        .env("LAZYXRP_DATA", data)
        .env_remove("XRPL_SEED")
        .env_remove("XRPL_MNEMONIC");
    (command, root)
}

#[test]
fn lazyxrp_binary_rejects_exec_without_subcommand() {
    let (mut command, _root) =
        command_with_isolated_env(env!("CARGO_BIN_EXE_lazyxrp"), "missing-subcommand");
    let output = command.arg("-x").output().expect("run lazyxrp binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a script subcommand"), "{stderr}");
}

#[test]
fn lazyxrp_binary_executes_info_against_local_rpc() {
    let (rpc_url, server) = spawn_rpc(
        r#"{"info":{"validated_ledger":{"seq":80000000},"hostid":"smoke","build_version":"local"}}"#,
    );
    let (mut command, _root) = command_with_isolated_env(env!("CARGO_BIN_EXE_lazyxrp"), "info");
    let output = command
        .args([
            "--allow-insecure-rpc",
            "--server",
            &rpc_url,
            "--ws-server",
            "ws://127.0.0.1:1",
            "-x",
            "info",
        ])
        .output()
        .expect("run lazyxrp info");
    server.join().expect("RPC server thread");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"ledger_index\": 80000000"), "{stdout}");
    assert!(stdout.contains("\"hostid\": \"smoke\""), "{stdout}");
}

#[test]
fn rp_binary_queries_account_against_local_rpc() {
    let (rpc_url, server) = spawn_rpc(
        r#"{"account_data":{"Account":"rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh","Balance":"10000000","Sequence":7,"OwnerCount":0,"Flags":0}}"#,
    );
    let (mut command, _root) = command_with_isolated_env(env!("CARGO_BIN_EXE_rp"), "rp-account");
    let output = command
        .args(["--allow-insecure-rpc", "--server", &rpc_url, ACCOUNT])
        .output()
        .expect("run rp binary");
    server.join().expect("RPC server thread");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"balance_xrp\": \"10.000000\""),
        "{stdout}"
    );
}
