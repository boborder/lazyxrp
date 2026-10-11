# maintenance.md

**Role:** 運用 — リリース・公開フロー。開発コマンドは [`tech.md`](tech.md) §5、アプリ動作は [`architecture.md`](architecture.md)、脅威は [`security.md`](security.md)。

## 1. 公開（リリース）フロー

公開は tag 駆動の自動リリース。`Cargo.toml` の `version` が SSOT。

**CI による `Cargo.toml` version 検知（[ci.yml](../.github/workflows/ci.yml)、2 か所）:**

- `release-gate`（Version & Lock Consistency）: 全 push / PR で `Cargo.toml` と `Cargo.lock` の `version` 一致を検査し、不一致ならフルテストマトリクス実行前に fail-fast。**version bump は `Cargo.lock` 同期と同一コミット**で push すること。
- `auto-tag`（Auto Tag Release）: `main` push で test / msrv / rustfmt / clippy / docs / security-advisories 全ジョブ緑の後、`Cargo.toml` の `version` を最新 `v*` タグと比較し、上がっていれば `v<version>` タグを作成・push。GITHUB_TOKEN のタグ push は他 workflow を発火させないため、続けて `gh workflow run CD --ref v<version>` で CD を明示 dispatch する（workflow 内コメントに記載の既知挙動）。
- `msrv`: `cargo +1.94.1 check --locked --all-targets`。`Cargo.toml` の `rust-version` を上げるときはこの job も同時に更新する。

**CD（公開、[cd.yml](../.github/workflows/cd.yml) — tag push / workflow_dispatch で起動）:**

| Job | 内容 |
|---|---|
| `ci-gate` | タグ付きコミットの CI run を完了まで監視し、非緑ならリリース中止 |
| `build-binaries` | 6 ターゲット（macOS x86_64/arm64、Linux x86_64/aarch64/i686、Windows x86_64 MSVC）を `--locked --release` でビルド。`lazyxrp-<tag>-<os>-<arch>.tar.gz` + `.sha256`（`rp` 同梱） |
| `publish-github-release` | tarball 一式を GitHub Release へ添付。本文 = 前タグからのコミット件名（`git log`）+ GitHub 生成ノート（`.github/release.yml` の PR ラベル分類） |
| `publish-cargo` | `cargo package --locked`（token なしで検証）→ `cargo publish --locked --no-verify`（crates.io、`CARGO_REGISTRY_TOKEN`）。アーカイブ内容は `Cargo.toml` の `include`（crates.io allowlist）が決定 |
| `trigger-benchmark` | リリース成功後、Benchmark workflow を `--ref v<version>` で dispatch |

**手動フォールバック:** 自動経路不調時のみ `mise run tag-push`（lint / test / doc / audit の 4 ゲート → `v<version>` タグ作成・push。tag push 自体で `cd.yml` が起動する）。

**リリース前（write 経路を変えた場合は必須）:** `mise run e2e-testnet`（`tests/testnet_e2e.rs`、公開 faucet + testnet で send / offer-create / offer-cancel を validated まで確認、約 30 秒）。CI では実行しない。0.3.0 では mock と実ノードの `simulate` 応答の差で全 write が壊れていた（TC-200）。

**変更履歴:** 利用者に見える変更は、その変更のコミットで `CHANGELOG.md` の `## [Unreleased]` に追記する。

**通常リリース手順:** `mise run release <x.y.z>`（main・clean worktree・未使用タグ・`[Unreleased]` の存在を検査 → `Cargo.toml` bump + `Cargo.lock` 同期 + `CHANGELOG.md` の `[Unreleased]` を `[x.y.z] - 日付` に確定し compare リンク更新 → lint / test → `chore: release <x.y.z>` を 1 コミット。push はしない）→ `git push` → CI 全緑 → auto-tag がタグ作成 → CD 公開（目安: push から crates.io まで約 11 分。内訳は CI 約 1 分、ビルド 4〜6.5 分、crates.io 公開 約 3.5 分）。手動タグは auto-tag が重複を検知して skip するため原則不要。

**crates.io パッケージ:** `Cargo.toml` の `include` が allowlist（`src/`、`tests/`、`build.rs`、`config.json5`、`DESIGN.md`（TC-128 が読む）、`CHANGELOG.md`、`SECURITY.md`、`LICENSE`、`README.md`、`.env.example`）。`docs/` は GitHub のみ。確認: `cargo package --list`（警告 0 であること）。

**定期 CI:** 毎週月曜 00:00 UTC に CI 全体を実行し、yanked crate や新しい advisory を push 前に検出する（2026-10-03 の yanked `yoke-derive` は push 時に初めて落ちた）。auto-tag は push 時のみ動く。

**依存更新:** [`.github/dependabot.yml`](../.github/dependabot.yml) が週次で PR を作る。GitHub Actions はコミット SHA 固定（`# vN` コメント付き）なので、その更新もこの PR 経由。cargo は minor/patch をまとめた 1 PR、major は個別。
