# AGENTS.md

**lazyxrp** — Elm-style unidirectional flow (`Action` → `App` → panels); network I/O under `xrpl/`, UI under `components/`. Human door: [`README.md`](README.md).

## Progressive disclosure

**Doc map SSOT — do not duplicate this table elsewhere.**

| Doc | Question |
|-----|----------|
| [`docs/requirements.md`](docs/requirements.md) | **何をするか** — 要件定義・ゴール設計 |
| [`docs/architecture.md`](docs/architecture.md) | **どう動くか** — 詳細設計・スキーマ設計 |
| [`docs/test.md`](docs/test.md) | **どう検証するか** — テスト設計・テストガイド |
| [`ROADMAP.md`](ROADMAP.md) | **何を・いつ・どこまで** — 進捗・目標・バージョン |
| [`docs/roadmap-log.md`](docs/roadmap-log.md) | **なぜ方向が変わったか** — 進路変更記録・計画上の注意点 |
| [`docs/tech.md`](docs/tech.md) | **何で作るか** — 技術選定・依存・API 連携 |
| [`docs/maintenance.md`](docs/maintenance.md) | **どう公開・運用するか** — リリース・公開フロー |
| [`docs/directory.md`](docs/directory.md) | **どこにあるか** — ディレクトリ構成・命名 |
| [`docs/security.md`](docs/security.md) | **どう守るか** — セキュリティ・機密管理 |
| [`docs/problems.md`](docs/problems.md) | **何に注意するか** — 注意点・落とし穴 |
| [`docs/references.md`](docs/references.md) | **調べる前に見る** — 参考文献・サンプル |
| [`DESIGN.md`](DESIGN.md) | **どう見える・操作するか** — UI/UX |
| **AGENTS.md** *(here)* | **どう作る・運用するか** — Agent 指示（最小） |

When shipping behavior: update root `ROADMAP.md` and `docs/` (`architecture.md`, `test.md`, …) in the same change.

**Bilingual docs:** every `docs/*.md` and `ROADMAP.md` is English and canonical; its Japanese twin is `*-ja.md` (e.g. `docs/architecture-ja.md`, `ROADMAP-ja.md`). Change both in the same change and keep the ID sets (TC / FR / NFR / S / R / I / P / F) identical. The `ROADMAP.md` coverage table exists only in the English file (`src/coverage.rs` TC-248 reads it). `DESIGN.md`, `README.md`, `AGENTS.md` and `CHANGELOG.md` have a single version.

## Execution contract

1. Missing **target**, **reproduction**, or **completion criteria** → ask **1–2** focused questions before implementing.
2. Otherwise implement.
3. After implementation → run **`cargo check`** (minimum); `mise run lint` is the CI lint gate (fmt + clippy).
4. After `cargo check` → sync related **`docs/`**.
5. Changes to **`src/config/`** keys or behavior → update every **`docs/`** file that mentions them in the same change.
6. Parallel agents: one integrator edits shared files (`CHANGELOG.md`, the `ROADMAP.md` coverage table, README counts); regenerate the table with `lazyxrp -x coverage`, never by hand.
7. Dependency updates → `mise run update` (cargo update + check + audit).

If assumptions are unavoidable, state them explicitly before proceeding.

## Critical overrides

- Components never call `xrpl/` clients/`poll`/`app` — `Action` flow only.
- Never sign/submit without simulate; mainnet writes require `--yes`.
- Use `SigningCredential` via `credential_from_secrets` (family seed **or** BIP39 mnemonic, not both). Cleared plaintext `seed`/`mnemonic` fields are invalid. Never mutate shared `ArcValue` JSON.
- No unrelated refactors; do not remove features to bypass errors; do not contradict `docs/`.
- Live-network checks (RPC state, fees): `ctx_execute` fetch; the shell hook rejects raw HTTP.

## Detailed instructions

- Graphify (optional, local only): `graphify-out/` is gitignored and not committed — generate it with `graphify update .`, then read `graphify-out/GRAPH_REPORT.md` for structure queries; regenerate when `HEAD` differs or `graphify-out/needs_update` exists.
- Domain skills: [`.agents/skills/xrpl-rust/`](.agents/skills/xrpl-rust/SKILL.md) · Flare: `flare-general` / `flare-ftso` / `flare-fassets` / `flare-fdc` / `flare-smart-accounts` (vendored from `flare-foundation/flare-ai-skills`; refresh with `npx skills update -p -y`, then restore any local-only `flare-fassets/scripts/_*` probes)
