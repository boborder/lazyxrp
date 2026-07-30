# Decide functional-improvement intake for the plan

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by:

## Question

機能改善も計画に含めてよい、という前提の **intake 基準** を決める。

決めること:

1. 「ratatui 活用・最適化」に同梱してよい改善の種類（例: フォーカス/Esc の一貫性、空状態表示、スクロール発見性）
2. 除外する種類（例: 新 RPC、新 TX フロー、設定スキーマ追加）— Out of scope の新パネルと整合
3. 改善を計画フェーズのどこに載せるか（shared / theme / split のどれに紐づくか）
4. 後続の具体候補リストは fog からこの基準で卒業させる、という運用でよいか

## Answer

- **同梱可:** フォーカス/Esc・空状態・スクロール発見性など、既存パネルの UX 一貫性改善。
- **除外:** 新 RPC・新 TX フロー・設定スキーマ追加（新パネル除外と整合）。
- **紐づけ:** 触るフェーズに紐づける（shared=空状態/スクロール、theme=見た目、split=フォーカス境界など）。横断物は最終計画の別バケット可。
- **運用:** 具体候補リストはこの基準で fog から卒業させ、別チケット/計画へ載せる（このチケットでは基準のみ）。

## Comments

- Graduated fog item "機能改善の具体候補リスト" → issue 09.
