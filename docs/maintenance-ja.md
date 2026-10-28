# maintenance.md

[English](maintenance.md)

**Role:** 運用 — リリース・公開フロー。開発コマンドは [`tech-ja.md`](tech-ja.md) §5、アプリ動作は [`architecture-ja.md`](architecture-ja.md)、脅威は [`security-ja.md`](security-ja.md)。

## 1. 公開（リリース）フロー

公開は tag 駆動の自動リリース。`Cargo.toml` の `version` が SSOT。

**CI による `Cargo.toml` version 検知（[ci.yml](../.github/workflows/ci.yml)、2 か所）:**

- `release-gate`（Version & Lock Consistency）: 全 push / PR で `Cargo.toml` と `Cargo.lock` の `version` 一致を検査し、不一致ならフルテストマトリクス実行前に fail-fast。**version bump は `Cargo.lock` 同期と同一コミット**で push すること。
- `auto-tag`（Auto Tag Release）: `main` push で test / msrv / rustfmt / clippy / docs / security-advisories / package / check-os の全ジョブが成功したあと、`Cargo.toml` の `version` を最新 `v*` タグと比較し、上がっていれば `v<version>` タグを作成・push。GITHUB_TOKEN のタグ push は他 workflow を発火させないため、続けて `gh workflow run CD --ref v<version>` で CD を明示 dispatch する（workflow 内コメントに記載の既知挙動）。
- `package`: `cargo package --locked`（`include` allowlist だけで crates.io 配布物がビルドできることを PR 段階で検証）。`check-os`: macOS / Windows で `cargo check --locked --all-targets`（リリース対象 OS のコンパイル確認）。どちらも auto-tag の `needs`。
- ワークフローは top-level `permissions` を最小（`contents: read`、cd.yml は `actions: read` も）にし、書き込みが必要な job（auto-tag / publish-github-release / trigger-benchmark）だけが個別に昇格する。push しない checkout は `persist-credentials: false`。
- `msrv`: `cargo +1.99 check --locked --all-targets`。`Cargo.toml` の `rust-version` を上げるときはこの job も同時に更新する。

**CD（公開、[cd.yml](../.github/workflows/cd.yml) — tag push / workflow_dispatch で起動）:**

| Job | 内容 |
|---|---|
| `ci-gate` | タグ付きコミットの CI run を完了まで監視し、非緑ならリリース中止 |
| `build-binaries` | 6 ターゲット（macOS x86_64/arm64、Linux x86_64/aarch64/i686、Windows x86_64 MSVC）を `--locked --release` でビルド。`lazyxrp-<tag>-<os>-<arch>.tar.gz` + `.sha256`（`rp` 同梱） |
| `publish-github-release` | tarball 一式を GitHub Release へ添付。本文 = 前タグからのコミット件名（`git log`）+ GitHub 生成ノート（`.github/release.yml` の PR ラベル分類） |
| `publish-cargo` | `cargo package --locked`（token なしで検証）→ `cargo publish --locked --no-verify`（crates.io、`CARGO_REGISTRY_TOKEN`）。アーカイブ内容は `Cargo.toml` の `include`（crates.io allowlist）が決定 |
| `trigger-benchmark` | リリース成功後、Benchmark workflow を `gh workflow run Benchmark -R "$GITHUB_REPOSITORY" --ref v<version>` で dispatch（checkout 無しの job なので `-R` 明示）。`workflow_run` 待ち受けは使わない: GITHUB_TOKEN 由来の CD では 5 回連続（v0.2.3–v0.2.7）で発火しなかった |

**手動フォールバック:** 自動経路不調時のみ `mise run tag-push`（lint / test / doc / audit の 4 ゲート → `v<version>` タグ作成・push。tag push 自体で `cd.yml` が起動する）。

**リリース前（write 経路を変えた場合は必須）:** `mise run e2e-testnet`（`tests/testnet_e2e.rs`、公開 faucet + testnet で send / offer-create / offer-cancel / NFT mint → burn を validated まで確認、2 テストは `--test-threads=1` で直列、約 40 秒）。testnet の open-ledger fee が高騰すると simulate が `exceeds the requested tx limit` で失敗する。`fee` RPC で確認し（`ctx_execute` の fetch）、`LAZYXRP_E2E_NETWORK=devnet mise run e2e-testnet` で代替する（docs/test-ja.md の run note）。CI では実行しない。0.3.0 では mock と実ノードの `simulate` 応答の差で全 write が使えなくなっていた（TC-200）。

**Benchmark:** `benchmark.sh`（`--json` / `--ci` / `--fast`、`mise run bench` / `bench-fast` / `bench-ci`）がビルド・check・test・clippy・doc・起動時間（hyperfine）・バイナリサイズ（cargo-bloat）・`cargo --timings` を個別 timeout で独立実行する（1 件の失敗が他を止めるのは `--ci` のときだけ）。`.github/workflows/benchmark.yml` は `workflow_dispatch` のみ（main push / PR では走らせない）。手動 dispatch の `strict=true` は `--ci` を渡す。成果物 `benchmark-results`（`benchmark-summary.json`・`benchmark.log`・`target/bloat-report.txt`・`target/timings-report.html`）。`--json` は `{benchmarks:[{name,result,value,unit,message}], summary:{pass,fail,skip,timestamp}, cache_comparison}`、`result` は `PASS` / `FAIL` / `TIMEOUT` / `SKIP`。

**変更履歴:** 利用者に見える変更は、その変更のコミットで `CHANGELOG.md` の `## [Unreleased]` に追記する。

**ドキュメントは二言語:** `docs/*.md` と `ROADMAP.md` はすべて日本語の対 `*-ja.md` を持つ。エージェントが参照する原文は英語版。片方を変更したら同じ変更でもう片方も変更し、ID 集合（TC/FR/NFR/S/R/I/P/F）を一致させる。自動チェックはないため、編集時に `X.md` と `X-ja.md` の ID 集合を比較する（例: 両方に `grep -o "TC-[0-9]*"`）。

**通常リリース手順:** `mise run release <x.y.z>`（main・clean worktree・未使用タグ・`[Unreleased]` に項目があることを検査 → `Cargo.toml` bump + `Cargo.lock` 同期 + `CHANGELOG.md` の `[Unreleased]` を `[x.y.z] - 日付` に確定し compare リンク更新 → lint / test → `chore: release <x.y.z>` を 1 コミット。push はしない）→ `git push` → CI 全緑 → auto-tag がタグ作成 → CD 公開（目安: push から crates.io まで約 11 分。内訳は CI 約 1 分、ビルド 4〜6.5 分、crates.io 公開 約 3.5 分）。手動タグは auto-tag が重複を検知して skip するため原則不要。

**crates.io パッケージ:** `Cargo.toml` の `include` が allowlist（`src/`、`tests/`、`build.rs`、`config.json5`、`DESIGN.md`（TC-128 が読む）、`CHANGELOG.md`、`SECURITY.md`、`LICENSE`、`README.md`、`ROADMAP.md`（TC-248 が読む）、`.env.example`）。`docs/` は GitHub のみ。確認: `cargo package --list`（警告 0 であること）。

**定期 CI:** 毎週月曜 00:00 UTC に CI 全体を実行し、yanked crate や新しい advisory を push 前に検出する（2026-10-03 の yanked `yoke-derive` は push 時に初めて失敗した）。auto-tag は push 時のみ動く。

**依存更新:** [`.github/dependabot.yml`](../.github/dependabot.yml) が週次で PR を作る。GitHub Actions はコミット SHA 固定（`# vN` コメント付き）なので、その更新もこの PR 経由。cargo は minor/patch をまとめた 1 PR、major は個別。例外: `tokio-tungstenite >=0.25` は dependabot で ignore、かつ `deny.toml` の `[bans] deny`（`tungstenite` / `tokio-tungstenite` `>=0.25`）で fail し、xrpl-rust の 0.24 ツリーとの二重化を防ぐ。xrpl-rust が上げたら両方外す。
