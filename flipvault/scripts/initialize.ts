// Initialize FlipVault on the configured cluster. Overridable via env.
import { BN } from "@coral-xyz/anchor";
import { SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { program, provider, configPda, reservePda, treasuryPda, vaultPda } from "./lib";

const SEED_SOL = Number(process.env.SEED_SOL ?? 1 * LAMPORTS_PER_SOL);
const INIT_RTOK = Number(process.env.INIT_RTOK ?? 1_000_000_000);
const ROUND_SECS = Number(process.env.ROUND_SECS ?? 30);
const FEE_BPS = Number(process.env.FEE_BPS ?? 1000); // 10%
const MIN_RESERVE = Number(process.env.MIN_RESERVE ?? 1_000_000); // 0.001 SOL

(async () => {
  const treasuryAuthority = provider.wallet.publicKey; // fee recipient
  console.log("Initializing with:", { SEED_SOL, INIT_RTOK, ROUND_SECS, FEE_BPS, MIN_RESERVE });
  const sig = await program.methods
    .initialize(
      new BN(SEED_SOL),
      new BN(INIT_RTOK),
      new BN(ROUND_SECS),
      FEE_BPS,
      new BN(MIN_RESERVE),
      treasuryAuthority
    )
    .accountsPartial({
      founder: provider.wallet.publicKey,
      config: configPda,
      reserve: reservePda,
      treasury: treasuryPda,
      vault0: vaultPda(0),
      vault1: vaultPda(1),
      vault2: vaultPda(2),
      vault3: vaultPda(3),
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("initialized:", sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
