// Shared client setup + PDA helpers for FlipVault scripts.
// Run with anchor env vars set, e.g.:
//   ANCHOR_PROVIDER_URL=https://api.devnet.solana.com \
//   ANCHOR_WALLET=/root/.config/solana/devnet.json \
//   npx ts-node scripts/<name>.ts
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { Flipvault } from "../target/types/flipvault";

export const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
export const program = anchor.workspace.flipvault as Program<Flipvault>;
export const connection = provider.connection;
export const pid = program.programId;

const pda = (seeds: (Buffer | Uint8Array)[]) =>
  PublicKey.findProgramAddressSync(seeds, pid)[0];

export const configPda = pda([Buffer.from("config")]);
export const reservePda = pda([Buffer.from("reserve")]);
export const treasuryPda = pda([Buffer.from("treasury")]);
export const vaultPda = (id: number) => pda([Buffer.from("vault"), Buffer.from([id])]);
export const positionPda = (owner: PublicKey, vaultId: number, slot: number) =>
  pda([Buffer.from("position"), owner.toBuffer(), Buffer.from([vaultId]), Buffer.from([slot])]);

// ORAO VRF (same address on devnet and mainnet).
export const ORAO_VRF_ID = new PublicKey("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
export const oraoNetworkState = () =>
  PublicKey.findProgramAddressSync([Buffer.from("orao-vrf-network-configuration")], ORAO_VRF_ID)[0];
export const oraoRandomness = (seed: Uint8Array) =>
  PublicKey.findProgramAddressSync(
    [Buffer.from("orao-vrf-randomness-request"), Buffer.from(seed)],
    ORAO_VRF_ID
  )[0];
