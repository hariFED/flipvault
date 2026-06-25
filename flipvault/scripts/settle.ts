// One-shot: settle the current pending round (return config to Idle) without committing a new one.
import { program, configPda, reservePda, vaultPda, oraoRandomness } from "./lib";

(async () => {
  const cfg = await program.account.config.fetch(configPda);
  if (Object.keys(cfg.phase)[0] !== "pending") {
    console.log("not pending; nothing to settle. phase:", Object.keys(cfg.phase)[0]);
    return;
  }
  const seed = Uint8Array.from(cfg.roundSeed as number[]);
  const random = oraoRandomness(seed);
  const sig = await program.methods
    .settleRound()
    .accountsPartial({
      config: configPda,
      reserve: reservePda,
      vault0: vaultPda(0),
      vault1: vaultPda(1),
      vault2: vaultPda(2),
      vault3: vaultPda(3),
      random,
    })
    .rpc();
  console.log("settled round:", sig);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
