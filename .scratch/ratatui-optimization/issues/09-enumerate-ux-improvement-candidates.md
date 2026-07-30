# Enumerate UX improvement candidates for the plan

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by: 06

## Question

intake 基準（06）に沿い、最終実装計画に載せる **具体的な UX 改善候補** を列挙・採否する。

範囲の例（確定ではない）:

- フォーカス / Esc の一貫性
- 空状態・ローディング表示の揃え
- スクロールバー発見性 / thumb 色統一
- その他、触るパネルに紐づく小さな一貫性改善

除外（06）: 新 RPC・新 TX フロー・設定スキーマ・新パネル。

## Answer

### Adopt（計画に載せる）

| 候補 | 紐づくフェーズ | メモ |
|------|----------------|------|
| Scrollbar thumb 色統一 | Phase-1 shared | **選択テーブルは `accent` に統一**（tx helper の secondary を合わせる） |
| スクロールバー発見性 | Phase-1 shared | ヘルパ側で一貫（常時/閾値は実装時に既存挙動互換を優先） |
| 空状態・ローディング・枠の揃え | Phase-3 theme（触った箇所） | `titled_block` / `panel_block` / 直書き Block の寄せ |
| フォーカス/Esc 一貫性（触る範囲） | 触るパネルのフェーズに紐づけ | 新フローは作らない；分割フェーズには混ぜない（07） |
| Row キャッシュ | **計画注記のみ** | dirty-render 後にまだ痛ければ別判断。Phase 本体には載せない |

### Reject / Out

- 新 RPC・新 TX・設定スキーマ・新パネル（06）
