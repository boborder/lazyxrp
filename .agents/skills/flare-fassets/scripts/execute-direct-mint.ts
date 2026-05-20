/**
 * Direct Mint executor skeleton (Coston2 default) — viem + FDC proof
 *
 * Modes (EXECUTOR_MODE):
 *   execute — fetch FDC Payment proof and call AssetManager.executeDirectMinting
 *   watch   — poll DirectMintingExecuted / DirectMintingDelayed on AssetManagerFXRP
 *
 * Write: sends a Flare tx when EXECUTOR_MODE=execute and DRY_RUN=false.
 *
 * Prerequisites:
 *   cd this directory && npm install
 *
 * Environment:
 *   EXECUTOR_MODE              — "execute" (default) | "watch"
 *   FLARE_RPC_URL              — default Coston2
 *   PRIVATE_KEY                — executor wallet (required if DRY_RUN=false in execute mode)
 *   DRY_RUN                    — default true; set "false" to submit executeDirectMinting
 *   XRPL_TX_HASH               — XRPL payment tx id (hex, with or without 0x); required for execute
 *   VOTING_ROUND_ID            — FDC voting round for proof; required for execute
 *   COSTON2_DA_LAYER_URL       — FDC DA API base URL (trailing slash ok)
 *   VERIFIER_URL_TESTNET       — FDC verifier base URL
 *   VERIFIER_API_KEY_TESTNET   — FDC API key
 *   WATCH_FROM_BLOCK           — watch mode: start block (default: latest - 5000)
 *   POLL_INTERVAL_MS           — watch mode sleep (default 15000)
 *
 * Usage:
 *   npx tsx execute-direct-mint.ts
 *   EXECUTOR_MODE=watch npx tsx execute-direct-mint.ts
 *
 * See: docs/external/fassets-direct-mint-monitoring.md
 *      https://dev.flare.network/fassets/developer-guides/fassets-direct-minting
 */

import {
  createPublicClient,
  createWalletClient,
  decodeEventLog,
  http,
  type Address,
  type Hex,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";

import { normalizeTxId, resolveWatchStartBlock } from "./execute-direct-mint.helpers.js";

const FLARE_CONTRACTS_REGISTRY = "0xaD67FE66660Fb8dFE9d6b1b4240d8650e30F6019" as const;

const REGISTRY_ABI = [
  {
    type: "function",
    name: "getContractAddressByName",
    stateMutability: "view",
    inputs: [{ name: "name", type: "string" }],
    outputs: [{ type: "address" }],
  },
] as const;

const ASSET_MANAGER_ABI = [
  {
    type: "function",
    name: "executeDirectMinting",
    stateMutability: "nonpayable",
    inputs: [
      {
        name: "_proof",
        type: "tuple",
        components: [
          { name: "merkleProof", type: "bytes32[]" },
          { name: "data", type: "bytes" },
        ],
      },
    ],
    outputs: [{ type: "uint256" }],
  },
  {
    type: "function",
    name: "getDirectMintingOthersCanExecuteAfterSeconds",
    stateMutability: "view",
    inputs: [],
    outputs: [{ type: "uint256" }],
  },
  {
    type: "function",
    name: "directMintingPaymentAddress",
    stateMutability: "view",
    inputs: [],
    outputs: [{ type: "string" }],
  },
  {
    type: "event",
    name: "DirectMintingExecuted",
    inputs: [
      { name: "minter", type: "address", indexed: true },
      { name: "agentVault", type: "address", indexed: true },
      { name: "mintedAmountUBA", type: "uint256", indexed: false },
    ],
  },
  {
    type: "event",
    name: "DirectMintingDelayed",
    inputs: [
      { name: "minter", type: "address", indexed: true },
      { name: "agentVault", type: "address", indexed: true },
      { name: "executionAllowedAt", type: "uint256", indexed: false },
    ],
  },
] as const;

const coston2 = {
  id: 114,
  name: "Flare Coston2",
  nativeCurrency: { name: "C2FLR", symbol: "C2FLR", decimals: 18 },
  rpcUrls: {
    default: { http: ["https://coston2-api.flare.network/ext/bc/C/rpc"] },
  },
} as const;

type FdcProofResponse = {
  proof: Hex[];
  response: Hex;
};

async function resolveAssetManager(publicClient: ReturnType<typeof createPublicClient>) {
  const assetManager = await publicClient.readContract({
    address: FLARE_CONTRACTS_REGISTRY,
    abi: REGISTRY_ABI,
    functionName: "getContractAddressByName",
    args: ["AssetManagerFXRP"],
  });
  return assetManager as Address;
}

async function prepareAttestationRequest(
  transactionId: string,
  verifierUrl: string,
  apiKey: string,
): Promise<{ abiEncodedRequest: Hex }> {
  const base = verifierUrl.endsWith("/") ? verifierUrl : `${verifierUrl}/`;
  const url = `${base}verifier/xrp/Payment/prepareRequest`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-API-KEY": apiKey,
    },
    body: JSON.stringify({
      attestationType: "0x5061796d656e7400000000000000000000000000000000000000000000000000",
      sourceId: "0x7465737458525000000000000000000000000000000000000000000000000000",
      requestBody: {
        transactionId,
        inUtxo: "0",
        utxo: "0",
      },
    }),
  });
  if (!response.ok) {
    throw new Error(`prepareRequest failed: ${response.status} ${await response.text()}`);
  }
  return (await response.json()) as { abiEncodedRequest: Hex };
}

async function fetchFdcProof(
  roundId: number,
  requestBytes: Hex,
  daLayerUrl: string,
  apiKey: string,
): Promise<FdcProofResponse> {
  const base = daLayerUrl.endsWith("/") ? daLayerUrl : `${daLayerUrl}/`;
  const url = `${base}api/v0/fdc/get-proof-round-id-bytes`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-API-KEY": apiKey,
    },
    body: JSON.stringify({
      votingRoundId: roundId,
      requestBytes,
    }),
  });
  if (!response.ok) {
    throw new Error(`get-proof failed: ${response.status} ${await response.text()}`);
  }
  const body = (await response.json()) as { proof: Hex[]; response: Hex };
  return { proof: body.proof, response: body.response };
}

async function runExecute(dryRun: boolean) {
  const rpcUrl = process.env.FLARE_RPC_URL ?? coston2.rpcUrls.default.http[0];
  const txHashRaw = process.env.XRPL_TX_HASH;
  const roundRaw = process.env.VOTING_ROUND_ID;
  const daUrl = process.env.COSTON2_DA_LAYER_URL;
  const verifierUrl = process.env.VERIFIER_URL_TESTNET;
  const apiKey = process.env.VERIFIER_API_KEY_TESTNET;

  if (!txHashRaw) throw new Error("XRPL_TX_HASH is required for execute mode");
  if (!roundRaw) throw new Error("VOTING_ROUND_ID is required for execute mode");
  if (!daUrl || !verifierUrl || !apiKey) {
    throw new Error(
      "Required: COSTON2_DA_LAYER_URL, VERIFIER_URL_TESTNET, VERIFIER_API_KEY_TESTNET",
    );
  }

  const transactionId = normalizeTxId(txHashRaw);
  const votingRoundId = Number(roundRaw);
  if (!Number.isInteger(votingRoundId) || votingRoundId <= 0) {
    throw new Error(`Invalid VOTING_ROUND_ID: ${roundRaw}`);
  }

  const publicClient = createPublicClient({
    chain: coston2,
    transport: http(rpcUrl),
  });

  const assetManager = await resolveAssetManager(publicClient);
  const othersAfter = await publicClient.readContract({
    address: assetManager,
    abi: ASSET_MANAGER_ABI,
    functionName: "getDirectMintingOthersCanExecuteAfterSeconds",
  });
  const coreVault = await publicClient.readContract({
    address: assetManager,
    abi: ASSET_MANAGER_ABI,
    functionName: "directMintingPaymentAddress",
  });

  console.log("Network: Coston2 (chain 114)");
  console.log("AssetManagerFXRP:", assetManager);
  console.log("Core Vault XRPL:", coreVault);
  console.log("othersCanExecuteAfterSeconds:", othersAfter.toString());
  console.log("XRPL tx:", transactionId);
  console.log("FDC voting round:", votingRoundId);

  console.log("\nFetching FDC Payment proof...");
  const prepared = await prepareAttestationRequest(transactionId, verifierUrl, apiKey);
  const fdcProof = await fetchFdcProof(votingRoundId, prepared.abiEncodedRequest, daUrl, apiKey);

  const proofArg = {
    merkleProof: fdcProof.proof,
    data: fdcProof.response,
  };

  if (dryRun) {
    console.log("\n[DRY RUN] Would call executeDirectMinting with:");
    console.log("  merkleProof length:", fdcProof.proof.length);
    console.log("  response bytes length:", (fdcProof.response.length - 2) / 2);
    console.log("\nSet DRY_RUN=false and PRIVATE_KEY to submit.");
    return;
  }

  const privateKey = process.env.PRIVATE_KEY;
  if (!privateKey) throw new Error("PRIVATE_KEY is required when DRY_RUN=false");

  const account = privateKeyToAccount(
    (privateKey.startsWith("0x") ? privateKey : `0x${privateKey}`) as Hex,
  );
  const walletClient = createWalletClient({
    account,
    chain: coston2,
    transport: http(rpcUrl),
  });

  console.log("\nSubmitting executeDirectMinting from", account.address);
  const hash = await walletClient.writeContract({
    address: assetManager,
    abi: ASSET_MANAGER_ABI,
    functionName: "executeDirectMinting",
    args: [proofArg],
  });
  console.log("Flare tx hash:", hash);

  const receipt = await publicClient.waitForTransactionReceipt({ hash });
  console.log("Status:", receipt.status);

  for (const log of receipt.logs) {
    if (log.address.toLowerCase() !== assetManager.toLowerCase()) continue;
    try {
      const decoded = decodeEventLog({
        abi: ASSET_MANAGER_ABI,
        data: log.data,
        topics: log.topics,
      });
      console.log("Event:", decoded.eventName, decoded.args);
    } catch {
      // unrelated log on same contract
    }
  }
}

async function runWatch() {
  const rpcUrl = process.env.FLARE_RPC_URL ?? coston2.rpcUrls.default.http[0];
  const pollMs = Number(process.env.POLL_INTERVAL_MS ?? "15000");
  const fromBlockEnv = process.env.WATCH_FROM_BLOCK;

  const publicClient = createPublicClient({
    chain: coston2,
    transport: http(rpcUrl),
  });

  const assetManager = await resolveAssetManager(publicClient);
  const latest = await publicClient.getBlockNumber();
  const resolved = resolveWatchStartBlock(fromBlockEnv, latest);
  const fromBlock = resolved.fromBlock;
  if (resolved.kind === "clamped") {
    console.warn(
      `WATCH_FROM_BLOCK ${resolved.requested} is past chain head ${resolved.latest}; clamping to ${fromBlock}`,
    );
  }

  console.log("Watch mode — AssetManagerFXRP:", assetManager);
  console.log("From block:", fromBlock.toString(), "| poll:", pollMs, "ms");
  console.log("Events: DirectMintingExecuted, DirectMintingDelayed");
  console.log("Ctrl+C to stop.\n");

  let lastScanned = fromBlock - 1n;

  const scan = async () => {
    const head = await publicClient.getBlockNumber();
    if (head <= lastScanned) return;

    const logs = await publicClient.getContractEvents({
      address: assetManager,
      abi: ASSET_MANAGER_ABI,
      fromBlock: lastScanned + 1n,
      toBlock: head,
    });

    for (const entry of logs) {
      const ts = new Date().toISOString();
      if (entry.eventName === "DirectMintingExecuted") {
        console.log(`[${ts}] DirectMintingExecuted`, {
          block: entry.blockNumber?.toString(),
          tx: entry.transactionHash,
          args: entry.args,
        });
      } else if (entry.eventName === "DirectMintingDelayed") {
        console.log(`[${ts}] DirectMintingDelayed`, {
          block: entry.blockNumber?.toString(),
          tx: entry.transactionHash,
          args: entry.args,
          executionAllowedAt: entry.args.executionAllowedAt?.toString(),
        });
      }
    }

    lastScanned = head;
  };

  await scan();
  setInterval(() => {
    scan().catch((err) => console.error("watch error:", err));
  }, pollMs);
}

async function main() {
  const mode = (process.env.EXECUTOR_MODE ?? "execute").toLowerCase();
  const dryRun = process.env.DRY_RUN !== "false";

  if (mode === "watch") {
    await runWatch();
    return;
  }
  if (mode !== "execute") {
    throw new Error(`Unknown EXECUTOR_MODE: ${mode} (use execute or watch)`);
  }

  await runExecute(dryRun);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
