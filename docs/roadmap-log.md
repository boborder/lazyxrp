# Roadmap log — 進路変更記録・計画上の注意点

**Role:** [`ROADMAP.md`](../ROADMAP.md) の補助記録。目標の階層と優先順位は ROADMAP、ここには「なぜ方向が変わったか」と「計画上の注意点」を置く。コードレベルの落とし穴・既知問題は [`problems.md`](problems.md) が SSOT。

## Cautions（計画上の注意点）

- **セキュリティは非交渉。** mainnet write は simulate + `--yes`、seed/mnemonic は `credential_from_secrets` 経由のみ、config の world-readable 警告を外さない。採用（人気）追求のために緩めない。詳細は [`security.md`](security.md)。
- Mainnet C3 execute smoke は FDC verifier API key + 実資金が必要（外部依存。入手できるまで中期に置きっぱなしで可）。
- Keymap customization は `src/config.rs` を触る → AGENTS.md 契約により config キーに言及する全 `docs/` を同一変更内で同期。組み込みデフォルトは `config.json5`。
- OfferCancel（旧 F-02）は DEX trading に統合済み。個別タスクとしては追わない。

## Pivot log（進路変更記録）

- **2026-09-21** — ROADMAP を horizon 型（最終ゴール / 長期 / 中期 / 短期）へ再構成。最終ゴールを「XRPL エコシステム（dev/user）で人気の CLI/TUI ツール + セキュリティ維持」と明文化。短期 = DEX trading (P1) / CLI redesign (P2) / keymap customization (P3)。F-02 OfferCancel → DEX trading に統合、F-04 AMM を Later→P3。
- **2026-09-17** — 新機能より安定化を優先: Flare read-path 最適化・XRPL poll 効率化・status bar health（0.2.11）。
- **2026-09-12** — multichain 拡大（Xahau + Flare）と docs 階層再設計。`docs/` → `openspec/` の一方通行リンク規則を確立。
