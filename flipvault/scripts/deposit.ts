// Deposit SOL into a vault's current SOL tranche.
//   npx ts-node scripts/deposit.ts <vaultId> <slot> <lamports>
import { BN } from "@coral-xyz/anchor";
import { SystemProgram } from "@solana/web3.js";
import { program, provider, configPda, vaultPda, positionPda } from "./lib";

const vaultId = Number(process.argv[2] ?? 0);
const slot = Number(process.argv[3] ?? 0);
const amount = Number(process.argv[4] ?? 100_000_000); // 0.1 SOL

(async () => {
  const user = provider.wallet.publicKey;
  const sig = await program.methods
    .deposit(vaultId, slot, new BN(amount))
    .accountsPartial({
      user,
      config: configPda,
      vault: vaultPda(vaultId),
      position: positionPda(user, vaultId, slot),
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log(`deposited ${amount} lamports into vault${vaultId} slot${slot}:`, sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
