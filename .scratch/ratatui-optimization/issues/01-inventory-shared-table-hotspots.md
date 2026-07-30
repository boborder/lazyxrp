# Inventory shared-table duplication and draw hotspots

Type: task
Status: resolved
Labels: wayfinder:task
Blocked by:

## Question

実装方針を決める前に、現状の共有化候補と描画ホットスポットを棚卸しする。

具体的に記録すること（`.scratch/ratatui-optimization/assets/` に短いメモで可）:

1. `SelectableTableState` + `Scrollbar` + `header_row_style` / `panel_block` のコピー箇所（ファイルと行付近）
2. `draw()` 内で毎フレーム `Row`/`String` を作り直しているパネル
3. `wallet.rs` / `server.rs` の論理ブロック境界（分割候補の見出し）

これは決定そのものではなく、後続の共有 API / perf / split チケットをアンブロックする事実収集。

## Answer

棚卸し完了。詳細は [assets/01-inventory-shared-table-hotspots.md](../assets/01-inventory-shared-table-hotspots.md)。

要約:

1. **共有済み**: `SelectableTableState` + `widgets::render_tx_scroll_table`（tx_history 経路）。**ほぼ同一コピー**: book / trust_lines / ledger_objects / path_find / server dUNL / tabs/nft。oracle・flare_ftso はステートレス Table のみ。Scrollbar thumb は共有 tx helper だけ `secondary`、他は `accent`。`titled_block_with_count` は border スタイルを再実装。
2. **毎フレーム Row/String 再生成**: 上記テーブル全般（特に ledger_objects / nft の truncate collect、server dUNL の幅依存 label）。wallet・server の metrics/summary `Line` も毎 draw で再構築。
3. **分割境界**: wallet → composer logic/view・keygen popup・summary draw・Component 殻。server → validator detail・dUNL table・metrics・Component 殻。
