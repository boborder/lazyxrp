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
| [`docs/tech.md`](docs/tech.md) | **何で作るか** — 技術選定・依存・API 連携 |
| [`docs/directory.md`](docs/directory.md) | **どこにあるか** — ディレクトリ構成・命名 |
| [`docs/security.md`](docs/security.md) | **どう守るか** — セキュリティ・機密管理 |
| [`docs/problems.md`](docs/problems.md) | **何に注意するか** — 注意点・落とし穴 |
| [`docs/references.md`](docs/references.md) | **調べる前に見る** — 参考文献・サンプル |
| [`DESIGN.md`](DESIGN.md) | **どう見える・操作するか** — UI/UX |
| **AGENTS.md** *(here)* | **どう作る・運用するか** — Agent 指示（最小） |

## Optional: OpenSpec

`openspec/` is **optional** agent change-tracking. It may reference `docs/`; **`docs/` never links to `openspec/`** (one-way).

| Use | Path |
|-----|------|
| Active change checklists | `openspec/changes/<name>/tasks.md` |
| Archived changes | `openspec/changes/archive/` |
| Agent runtime contracts | `openspec/specs/<cap>/spec.md` |
| Hub config | `openspec/config.yaml` |

When shipping behavior: update root `ROADMAP.md` and `docs/` (`architecture.md`, `test.md`, …) in the same change. If OpenSpec was used: `openspec archive <name> -y`.

## Execution contract

1. Missing **target**, **reproduction**, or **completion criteria** → ask **1–2** focused questions before implementing.
2. Otherwise implement.
3. After implementation → run **`cargo check`** (minimum).
4. After `cargo check` → sync related **`docs/`**.
5. Changes to **`src/config.rs`** keys or behavior → update every **`docs/`** file that mentions them in the same change.

If assumptions are unavoidable, state them explicitly before proceeding.

## Critical overrides

- Components never call `xrpl/` clients/`poll`/`app` — `Action` flow only.
- Never sign/submit without simulate; mainnet writes require `--yes`.
- Use `SigningCredential` via `credential_from_secrets` (family seed **or** BIP39 mnemonic, not both). Cleared plaintext `seed`/`mnemonic` fields are invalid. Never mutate shared `ArcValue` JSON.
- No unrelated refactors; do not remove features to bypass errors; do not contradict `docs/`.
- Do not skip `cargo check` after implementation.

## Detailed instructions

- Graphify: [`graphify-out/GRAPH_REPORT.md`](graphify-out/GRAPH_REPORT.md) — if `HEAD` differs or `graphify-out/needs_update` exists, run `graphify update .` before structure queries.
- Domain skills: [`.agents/skills/xrpl-rust/`](.agents/skills/xrpl-rust/SKILL.md) · Flare: `flare-general` / `flare-ftso` / `flare-fassets`
