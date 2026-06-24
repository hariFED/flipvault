// Treasury authority sweeps accrued fees to itself.
//   npx ts-node scripts/sweep.ts <lamports>
import { BN } from "@coral-xyz/anchor";
import { program, provider, configPda, treasuryPda } from "./lib";

const amount = Number(process.argv[2] ?? 0);

(async () => {
  const sig = await program.methods
    .sweepTreasury(new BN(amount))
    .accountsPartial({
      authority: provider.wallet.publicKey,
      config: configPda,
      treasury: treasuryPda,
      recipient: provider.wallet.publicKey,
    })
    .rpc();
  console.log(`swept ${amount} lamports:`, sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
