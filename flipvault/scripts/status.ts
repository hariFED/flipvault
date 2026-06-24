// Print FlipVault on-chain state.
import { LAMPORTS_PER_SOL } from "@solana/web3.js";
import { program, connection, configPda, reservePda, treasuryPda, vaultPda } from "./lib";

(async () => {
  const cfg = await program.account.config.fetch(configPda);
  console.log("=== config ===");
  console.log("phase:        ", Object.keys(cfg.phase)[0]);
  console.log("r_tok:        ", cfg.rTok.toString());
  console.log("k:            ", cfg.k.toString());
  console.log("fee_bps:      ", cfg.feeBps);
  console.log("round_secs:   ", cfg.roundSecs.toString());
  console.log("min_reserve:  ", cfg.minReserve.toString());
  console.log("last_settled: ", cfg.lastSettledTs.toString());
  console.log("selected_vault:", cfg.selectedVault);
  console.log("treasury_auth:", cfg.treasuryAuthority.toBase58());

  const sol = (n: number) => (n / LAMPORTS_PER_SOL).toFixed(6) + " SOL";
  console.log("\n=== balances ===");
  console.log("reserve: ", sol(await connection.getBalance(reservePda)));
  console.log("treasury:", sol(await connection.getBalance(treasuryPda)));

  console.log("\n=== vaults ===");
  for (let i = 0; i < 4; i++) {
    const v = await program.account.vault.fetch(vaultPda(i));
    const tr = v.tranches.map(
      (t: any) =>
        `${Object.keys(t.asset)[0]}(amt=${t.amount.toString()}, shares=${t.totalShares.toString()})`
    );
    console.log(`vault${i}: [${tr.join(", ")}]  bal=${sol(await connection.getBalance(vaultPda(i)))}`);
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
