# C4 — Containers (Level 2)

lazyxrp は **1 つの Rust バイナリ**として配布される。実行時コンテナはコード境界に対応するが、それぞれ同一プロセス内で動く。

- **Watch（TUI）**: `App` / `ratatui` / `tokio` イベントループ（設計の §3）。
- **CLI ランナー**: `xrpl::execute_cli_command` 経由の非対話コマンド。
- **XRPL 統合**: `RpcClient`（`src/xrpl/client.rs`）、WebSocket（`ws.rs`）、`PollCommand` ポーリング（`poll.rs`）；入口は `src/xrpl/mod.rs` の再エクスポート。
- **設定・署名**: `Config`（既定 + `config.toml` マージ）、`SigningConfig` / シード（`src/config.rs`, `src/signing.rs`）。

コンポーネント粒度の図が必要になったら別途 Level 3 を切る。

```mermaid
C4Container
  title Container Diagram — lazyxrp

  Person(operator, "Operator / Developer", "Runs lazyxrp locally")

  System_Ext(xrpl_net, "XRPL public endpoints", "JSON-RPC + WebSocket")

  System_Boundary(sys, "lazyxrp (single process)") {
    Container(tui, "Watch mode (TUI)", "Rust, ratatui, tokio, crossterm", "Dashboard, panels, keyboard-driven flows")
    Container(cli, "CLI commands", "Rust, clap", "info, account, book, summary, …")
    Container(xrpl_int, "XRPL integration", "Rust, xrpl-rust", "RPC, WS subscription loop, poll task → Action channel")
    Container(cfg, "Config & signing", "Rust", "Merged config.toml; seed handling for signing")
  }

  Rel(operator, tui, "Uses", "terminal")
  Rel(operator, cli, "Invokes", "terminal")
  Rel(tui, xrpl_int, "Actions / PollCommand", "tokio mpsc")
  Rel(cli, xrpl_int, "Queries & submits", "async calls")
  Rel(tui, cfg, "Reads runtime settings & endpoints")
  Rel(cli, cfg, "Reads signing / XRPL options")
  Rel(xrpl_int, xrpl_net, "Ledger IO", "HTTPS JSON-RPC / WSS")
```
