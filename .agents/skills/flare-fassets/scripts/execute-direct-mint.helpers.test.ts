import { describe, expect, it } from "vitest";
import {
  normalizeTxId,
  resolveWatchStartBlock,
  WATCH_LOOKBACK_BLOCKS,
} from "./execute-direct-mint.helpers.js";

describe("normalizeTxId", () => {
  const valid =
    "abcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd";

  it("accepts 64 hex without 0x and uppercases", () => {
    expect(normalizeTxId(valid.toLowerCase())).toBe(valid.toUpperCase());
  });

  it("strips 0x prefix", () => {
    expect(normalizeTxId(`0x${valid}`)).toBe(valid.toUpperCase());
  });

  it("rejects wrong length", () => {
    expect(() => normalizeTxId("abcd")).toThrow(/64 hex/);
  });

  it("rejects non-hex", () => {
    const bad = valid.slice(0, 63) + "g";
    expect(() => normalizeTxId(bad)).toThrow(/64 hex/);
  });
});

describe("resolveWatchStartBlock", () => {
  it("defaults to latest - lookback when env empty", () => {
    const latest = 12_000n;
    expect(resolveWatchStartBlock(undefined, latest)).toEqual({
      kind: "ok",
      fromBlock: latest - WATCH_LOOKBACK_BLOCKS,
    });
  });

  it("defaults to 0 when latest <= lookback", () => {
    const latest = 100n;
    expect(resolveWatchStartBlock(undefined, latest)).toEqual({
      kind: "ok",
      fromBlock: 0n,
    });
  });

  it("uses explicit block when <= latest", () => {
    const latest = 20_000n;
    expect(resolveWatchStartBlock("7000", latest)).toEqual({
      kind: "ok",
      fromBlock: 7000n,
    });
  });

  it("clamps when requested > latest", () => {
    const latest = 10_000n;
    expect(resolveWatchStartBlock("99999999", latest)).toEqual({
      kind: "clamped",
      fromBlock: latest - WATCH_LOOKBACK_BLOCKS,
      requested: 99_999_999n,
      latest,
    });
  });

  it("clamps to 0 when latest small and requested > latest", () => {
    const latest = 100n;
    expect(resolveWatchStartBlock("99999999", latest)).toEqual({
      kind: "clamped",
      fromBlock: 0n,
      requested: 99_999_999n,
      latest,
    });
  });

  it("treats blank string like unset", () => {
    const latest = 12_000n;
    expect(resolveWatchStartBlock("   ", latest)).toEqual({
      kind: "ok",
      fromBlock: latest - WATCH_LOOKBACK_BLOCKS,
    });
  });
});

describe("resolveWatchStartBlock — invalid env", () => {
  it("throws on non-integer BigInt input", () => {
    expect(() => resolveWatchStartBlock("12.5", 100n)).toThrow();
  });
});
