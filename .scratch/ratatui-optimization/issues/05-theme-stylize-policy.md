# Decide theme and Stylize policy

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by:

## Question

テーマ方針: 現状の Royal Blue RGB パレット + `theme::*` ヘルパを、どこまで `Stylize` / セマンティック色に寄せるか？

決めること:

1. ブランド RGB を維持する範囲（border / accent / success / error 等）
2. `HIGHLIGHT_FG` の白など、テーマ非適応色を例外として残すか置換するか
3. パネル実装が直接 `Style::new().fg(...)` してよい条件 vs 必ず `theme::` 経由
4. 「完了」の定義（新規コード規約のみ / 既存パネルの一括寄せ / 段階移行）

## Answer

- **パレット:** 現行 Royal Blue / turquoise / success / error / warning RGB を維持。`theme::*` ヘルパ経由。Stylize は書き方の糖衣として可。
- **HIGHLIGHT_FG:** 白をブランド例外として残す（focused selection の白FG+青BG）。
- **直接 Style:** 色は必ず `theme::` 定数/ヘルパ経由。Modifier / Stylize の組み合わせは可。`flag_style` のワンオフ RGB も theme へ寄せる対象。
- **完了定義:** 段階移行 — 規約を固定し、新規コードと触った箇所から寄せる。全パネル一括リライトは必須にしない。

## Comments

- Grilling resolved against `src/components/shared/theme.rs` current palette.
