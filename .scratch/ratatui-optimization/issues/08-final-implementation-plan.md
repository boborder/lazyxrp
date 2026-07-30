# Draft final phased implementation plan

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by: 03, 04, 05, 07, 09

## Question

ここまでの決定を、実装セッションへ渡す **最終実装計画1本** にまとめる。置き場所は `.scratch/ratatui-optimization/implementation-plan.md`（このチケットの成果物）。

計画に必ず含めること:

1. Phase 順（共有 → perf → theme → split）と各 Phase の DoD
2. 触るファイルの初期セット / 触らないもの（Out of scope 再掲）
3. リスク（選択ロスト、端末テーマ、巨大 diff）と緩和
4. 検証コマンド（`cargo fmt` / `clippy` / 関連 test）
5. 未決のまま残す fog（あれば明示）

このチケットの解決 = map Destination 達成。本番コードはまだ変えない。

## Answer

承認済み。成果物: [implementation-plan.md](../implementation-plan.md)

追加確定（本チケット）:

- Phase-2 順: **book → server dUNL**
- FPS/Splash: **見た目変化時のみ dirty**
- 実装 PR: **単一 PR**（内部は Phase 順コミット推奨）

Destination 達成。本番コード未変更。
