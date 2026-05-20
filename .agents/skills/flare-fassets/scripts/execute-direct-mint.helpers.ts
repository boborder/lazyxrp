/**
 * Pure helpers for `execute-direct-mint.ts` — unit-tested (see `*.test.ts`).
 */

export const WATCH_LOOKBACK_BLOCKS = 5000n;

export function normalizeTxId(raw: string): string {
  const id = raw.trim().replace(/^0x/i, "").toUpperCase();
  if (!/^[0-9A-F]{64}$/.test(id)) {
    throw new Error(`XRPL_TX_HASH must be 64 hex chars, got: ${raw}`);
  }
  return id;
}

export type ResolveWatchStartBlockResult =
  | { kind: "ok"; fromBlock: bigint }
  | { kind: "clamped"; fromBlock: bigint; requested: bigint; latest: bigint };

/**
 * Resolve the first block to scan in watch mode.
 * If `fromBlockEnv` points past `latest`, clamp to `latest - WATCH_LOOKBACK_BLOCKS` or `0`.
 */
export function resolveWatchStartBlock(
  fromBlockEnv: string | undefined,
  latest: bigint,
): ResolveWatchStartBlockResult {
  if (fromBlockEnv === undefined || fromBlockEnv.trim() === "") {
    const fromBlock =
      latest > WATCH_LOOKBACK_BLOCKS ? latest - WATCH_LOOKBACK_BLOCKS : 0n;
    return { kind: "ok", fromBlock };
  }

  const requested = BigInt(fromBlockEnv.trim());
  if (requested > latest) {
    const fromBlock =
      latest > WATCH_LOOKBACK_BLOCKS ? latest - WATCH_LOOKBACK_BLOCKS : 0n;
    return { kind: "clamped", fromBlock, requested, latest };
  }

  return { kind: "ok", fromBlock: requested };
}
