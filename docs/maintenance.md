# maintenance.md

**Role:** 運用 — リリース・公開フロー。開発コマンドは [`tech.md`](tech.md) §5、アプリ動作は [`architecture.md`](architecture.md)、脅威は [`security.md`](security.md)。

## 1. 公開（リリース）フロー

公開は tag 駆動の自動リリース。`Cargo.toml` の `version` が SSOT。

**CI による `Cargo.toml` version 検知（[ci.yml](../.github/workflows/ci.yml)、2 か所）:**

- `release-gate`（Version & Lock Consistency）: 全 push / PR で `Cargo.toml` と `Cargo.lock` の `version` 一致を検査し、不一致ならフルテストマトリクス実行前に fail-fast。**version bump は `Cargo.lock` 同期と同一コミット**で push すること。
- `auto-tag`（Auto Tag Release）: `main` push で test / rustfmt / clippy / docs / security-advisories 全ジョブ緑の後、`Cargo.toml` の `version` を最新 `v*` タグと比較し、上がっていれば `v<version>` タグを作成・push。GITHUB_TOKEN のタグ push は他 workflow を発火させないため、続けて `gh workflow run CD --ref v<version>` で CD を明示 dispatch する（workflow 内コメントに記載の既知挙動）。

**CD（公開、[cd.yml](../.github/workflows/cd.yml) — tag push / workflow_dispatch で起動）:**

| Job | 内容 |
|---|---|
| `ci-gate` | タグ付きコミットの CI run を完了まで監視し、非緑ならリリース中止 |
| `build-binaries` | 6 ターゲット（macOS x86_64/arm64、Linux x86_64/aarch64/i686、Windows x86_64 MSVC）を `--locked --release` でビルド。`lazyxrp-<tag>-<os>-<arch>.tar.gz` + `.sha256`（`rp` 同梱） |
| `publish-github-release` | tarball 一式を GitHub Release へ添付 |
| `publish-cargo` | `cargo publish --locked`（crates.io、`CARGO_REGISTRY_TOKEN`）。アーカイブ内容は `Cargo.toml` の `include`（crates.io allowlist）が決定 |
| `trigger-benchmark` | リリース成功後、Benchmark workflow を `--ref v<version>` で dispatch |

**手動フォールバック:** 自動経路不調時のみ `mise run tag-push`（lint / test / doc / audit の 4 ゲート → `v<version>` タグ作成・push。tag push 自体で `cd.yml` が起動する）。

**通常リリース手順:** `version` bump + `Cargo.lock` 同期を同一コミットで `main` push（例: `chore: release 0.2.10`）→ CI 全緑 → auto-tag がタグ作成 → CD 公開。手動タグは auto-tag が重複検知して skip するため原則不要。
