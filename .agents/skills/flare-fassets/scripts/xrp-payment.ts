/**
 * Send XRP payment for FAssets minting — Skill resource script
 *
 * Flow: Connect to XRPL testnet → build Payment with memo → sign and submit
 * Write: sends a real XRP Ledger transaction; requires a funded XRPL wallet.
 *
 * Sends XRP to an FAssets agent's underlying address with the payment reference
 * from the collateral reservation step encoded in the memo field.
 *
 * Review this script before running; execute in an isolated environment.
 * Update the constants (AGENT_ADDRESS, AMOUNT_XRP, PAYMENT_REFERENCE)
 * with values from your collateral reservation.
 *
 * Environment: XRPL_SEED (required), XRPL_WS_URL (optional, default testnet)
 *
 * Prerequisites: npm install xrpl
 * For proper ABI usage and type safety in related FAssets scripts, use the Flare periphery packages:
 *   - Solidity contracts: https://www.npmjs.com/package/@flarenetwork/flare-periphery-contracts
 *   - Artifacts: https://www.npmjs.com/package/@flarenetwork/flare-periphery-contract-artifacts
 *   - Wagmi types: https://www.npmjs.com/package/@flarenetwork/flare-wagmi-periphery-package
 * Usage: npx ts-node scripts/xrp-payment.ts
 *
 * See: https://dev.flare.network/fassets/developer-guides/fassets-mint
 */

import { Client, Wallet, xrpToDrops } from "xrpl";
import type { Payment, TxResponse } from "xrpl";

// Update these with values from the collateral reservation step
const AGENT_ADDRESS = "r4KgCNzn9ZuNjpf17DEHZnyyiqpuj599Wm";
const AMOUNT_XRP = "10.025";
const PAYMENT_REFERENCE =
  "4642505266410001000000000000000000000000000000000000000000f655fb";

const PLACEHOLDER_SEEDS = new Set(["PUT_SEED_HERE", "sXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"]);

function loadWalletFromEnv(): Wallet {
  const seed = process.env.XRPL_SEED?.trim();
  if (!seed) {
    throw new Error("XRPL_SEED environment variable is required");
  }
  if (PLACEHOLDER_SEEDS.has(seed)) {
    throw new Error("XRPL_SEED is still a placeholder — set a funded testnet wallet seed");
  }
  return Wallet.fromSeed(seed);
}

async function main() {
  const wsUrl = process.env.XRPL_WS_URL ?? "wss://s.altnet.rippletest.net:51233";
  const client = new Client(wsUrl);
  await client.connect();

  const wallet = loadWalletFromEnv();

  const paymentTx: Payment = {
    TransactionType: "Payment",
    Account: wallet.classicAddress,
    Destination: AGENT_ADDRESS,
    Amount: xrpToDrops(AMOUNT_XRP),
    Memos: [
      {
        Memo: {
          MemoData: PAYMENT_REFERENCE,
        },
      },
    ],
  };

  console.log("Submitting payment:", paymentTx);

  const prepared = await client.autofill(paymentTx);
  const signed = wallet.sign(prepared);
  const result: TxResponse = await client.submitAndWait(signed.tx_blob);

  console.log("Transaction hash:", signed.hash);
  console.log("Explorer: https://testnet.xrpl.org/transactions/" + signed.hash);
  console.log("Result:", result);

  await client.disconnect();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
