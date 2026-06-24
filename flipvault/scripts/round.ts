// Keeper: run one full round — commit_round (requests ORAO VRF), wait for fulfillment,
// then settle_round (flips vault = rand % 4). Run after the round interval has elapsed.
import { Orao, randomnessAccountAddress, networkStateAccountAddress } from "@orao-network/solana-vrf";
import { Keypair, SystemProgram } from "@solana/web3.js";
import {
  program,
  provider,
  configPda,
  reservePda,
  vaultPda,
  ORAO_VRF_ID,
} from "./lib";

(async () => {
  const orao = new Orao(provider as any);
  const ns = await orao.getNetworkState();
  const oraoTreasury = ns.config.treasury;

  // 32-byte VRF seed.
  const force = Keypair.generate().publicKey.toBytes();
  const random = randomnessAccountAddress(force);
  const networkState = networkStateAccountAddress();

  console.log("commit: seed =", Buffer.from(force).toString("hex"));
  const commitSig = await program.methods
    .commitRound([...force])
    .accountsPartial({
      keeper: provider.wallet.publicKey,
      config: configPda,
      random,
      oraoTreasury,
      networkState,
      vrf: ORAO_VRF_ID,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("  ->", commitSig);

  // Poll ORAO until the randomness is fulfilled (typically sub-second on devnet).
  process.stdout.write("waiting for VRF");
  let ok = false;
  for (let i = 0; i < 60; i++) {
    try {
      const r = await orao.getRandomness(force);
      if (r.getFulfilledRandomness()) {
        ok = true;
        break;
      }
    } catch (_) {
      /* account not yet visible */
    }
    process.stdout.write(".");
    await new Promise((res) => setTimeout(res, 2000));
  }
  console.log("");
  if (!ok) {
    throw new Error(
      "VRF not fulfilled within ~2min. Once RECOVER_AFTER_SECS passes you can cancel via scripts/recover.ts."
    );
  }

  console.log("settle: flipping vault = rand % 4");
  const settleSig = await program.methods
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
  console.log("  ->", settleSig);

  const cfg = await program.account.config.fetch(configPda);
  console.log("settled. selected vault:", cfg.selectedVault);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
