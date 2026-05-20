# Decide shared selectable-table helper contract

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by: 01

## Question

共有ウィジェット整理の中核: `SelectableTableState` + 縦スクロールバー + ヘッダ行スタイルを、どの API 形のヘルパに寄せるか？

決めること:

1. モジュール配置（例: `components/shared/selectable_table.rs` 拡張 vs 新モジュール）
2. 呼び出し面（`render_table_with_scrollbar(frame, area, table, state)` のような関数か、小さな struct か）
3. Phase-1 で移行するパネル集合（最小セット）
4. カスタム差分（Book の BarChart 併置、Server の特殊スクロール等）をヘルパ外に残す境界

挙動フリーズはしないが、キー操作・選択モデルは現状互換をデフォルト前提にするかどうかも含めて決める。

## Answer

- **呼び出し面:** 関数 `render_*`（構築済み `Table` + `&mut SelectableTableState` を受け、layout + Scrollbar まで担当）。`render_tx_scroll_table` は薄いラッパ／呼び出し側に残す。
- **配置:** `components/shared/selectable_table.rs` を拡張（state と描画ヘルパを同居）。
- **Phase-1 移行セット:** `trust_lines` / `ledger_objects` / `path_find` / `nft` + 既存 tx ヘルパの統一。`book`（BarChart 併置）と `server` dUNL（幅依存 label）は Phase-2。
- **境界:** ヘルパは表本体のみ。併置 UI（Book chart、Server metrics/detail、oracle/FTSO ステートレス表、detail overlay）は呼び出し側。
- **互換:** キー操作・選択モデルは現状互換。見た目の小さな統一（例: scrollbar thumb accent vs secondary）は別レバーとして計画可。

## Comments

- Grilling resolved in session; inventory asset `assets/01-inventory-shared-table-hotspots.md` used as fact base.
