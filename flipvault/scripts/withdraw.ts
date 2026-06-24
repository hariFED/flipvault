// Withdraw shares from a vault's SOL tranche (90% to you, 10% fee to treasury).
//   npx ts-node scripts/withdraw.ts <vaultId> <slot> <shares>
import { BN } from "@coral-xyz/anchor";
import { program, provider, configPda, treasuryPda, vaultPda, positionPda } from "./lib";

const vaultId = Number(process.argv[2] ?? 0);
const slot = Number(process.argv[3] ?? 0);
const shares = Number(process.argv[4] ?? 0);

(async () => {
  const user = provider.wallet.publicKey;
  const sig = await program.methods
    .withdraw(vaultId, slot, new BN(shares))
    .accountsPartial({
      user,
      config: configPda,
      vault: vaultPda(vaultId),
      position: positionPda(user, vaultId, slot),
      treasury: treasuryPda,
    })
    .rpc();
  console.log(`withdrew ${shares} shares from vault${vaultId} slot${slot}:`, sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
