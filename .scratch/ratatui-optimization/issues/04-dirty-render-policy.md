# Decide dirty-render / frame policy

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by: 02

## Question

描画パフォーマンス方針: 現状の tick/render interval 駆動をどう変える（または変えない）か？

決めること:

1. 常時 `Event::Render` を維持するか、`needs_render`（または同等）+ 最小フレーム間隔に寄せるか
2. データ更新（`Action::Xrpl*`）・キー・リサイズそれぞれが再描画を起こすルール
3. デフォルト `frame_rate`（60）を下げるか、スロットルのみか
4. 受け入れ条件（主観スムーズさ / アイドル時 CPU 目安 / 計測方法の有無）

画像パスや TEA 全面改修は Out of scope。

## Answer

- **方針:** `needs_render` + min-frame duration（~16ms）へ寄せる。常時 ungated `Event::Render` 描画はやめる。
- **dirty 源:** キー入力・リサイズ・モーダル入力は常に dirty。`Tick` はスピナー等 UI が実際に変わるときのみ。`Action::Xrpl*` 等データ更新は状態が変わったときのみ。
- **frame_rate:** デフォルト 60 維持。上限スロットルのみ（idle は dirty 無しでほぼ描画しない）。
- **受け入れ:** 主観的なスムーズさ + 既存 FPS 表示で異常がないこと。数値ベンチ/アイドルCPU閾値は必須にしない。
- **残課題（fog）:** FPS カウンタ / Splash の dirty 例外経路の具体化。Row キャッシュは dirty-render 後の判断。

## Comments

- Grilling resolved; research asset `assets/02-ratatui-render-patterns.md` used as basis.
