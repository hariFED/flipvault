// Cancel a round stuck waiting on VRF (only works after RECOVER_AFTER_SECS since commit).
import { program, configPda } from "./lib";

(async () => {
  const sig = await program.methods
    .recoverRound()
    .accountsPartial({ config: configPda })
    .rpc();
  console.log("round cancelled (no flip):", sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
