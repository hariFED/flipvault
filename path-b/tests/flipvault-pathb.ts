import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { PublicKey } from "@solana/web3.js";
import { FlipvaultPathb } from "../target/types/flipvault_pathb";
import { randomBytes } from "crypto";
import {
  awaitComputationFinalization,
  getArciumEnv,
  getCompDefAccOffset,
  getArciumAccountBaseSeed,
  getArciumProgramId,
  getArciumProgram,
  uploadCircuit,
  RescueCipher,
  deserializeLE,
  getMXEPublicKey,
  getMXEAccAddress,
  getMempoolAccAddress,
  getCompDefAccAddress,
  getExecutingPoolAccAddress,
  getComputationAccAddress,
  getClusterAccAddress,
  getLookupTableAddress,
  x25519,
} from "@arcium-hq/client";
import * as fs from "fs";
import * as os from "os";
import { expect } from "chai";

// ---- transparent curve (mirrors programs/flipvault/.../curve.rs; the M0a oracle) ----
function ceilDiv(a: bigint, b: bigint): bigint {
  return a / b + (a % b !== 0n ? 1n : 0n);
}
function buy(rSol: bigint, rTok: bigint, k: bigint, dx: bigint) {
  if (dx === 0n) return { tokOut: 0n, newRSol: rSol, newRTok: rTok };
  const denom = rSol + dx;
  const raw = k / denom;
  const newRTok = raw > rTok ? rTok : raw;
  return { tokOut: rTok - newRTok, newRSol: denom, newRTok };
}

const GENESIS_R_SOL = 100_000_000_000n; // 100 SOL
const GENESIS_R_TOK = 100_000_000_000n;
const K = GENESIS_R_SOL * GENESIS_R_TOK; // 1e22
const FEE_BPS = 1000n; // 10%
const DEPOSIT = 5_000_000_000n; // 5 SOL of genesis box balance

describe("FlipVault Path-B", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.FlipvaultPathb as Program<FlipvaultPathb>;
  const provider = anchor.getProvider();
  const arciumProgram = getArciumProgram(provider as anchor.AnchorProvider);
  const arciumEnv = getArciumEnv();
  const clusterAccount = getClusterAccAddress(arciumEnv.arciumClusterOffset);

  const owner = readKpJson(`${os.homedir()}/.config/solana/id.json`);

  const configPda = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId)[0];
  const curvePda = PublicKey.findProgramAddressSync([Buffer.from("curve")], program.programId)[0];
  const treasuryPda = PublicKey.findProgramAddressSync([Buffer.from("treasury")], program.programId)[0];
  const boxPda = PublicKey.findProgramAddressSync(
    [Buffer.from("box"), owner.publicKey.toBuffer()],
    program.programId
  )[0];

  // The player's x25519 key (shared secret with the MXE) — genesis box sealed to this.
  const privateKey = x25519.utils.randomSecretKey();
  const publicKey = x25519.getPublicKey(privateKey);

  const arciumAccounts = (offset: anchor.BN, compDefName: string) => ({
    computationAccount: getComputationAccAddress(arciumEnv.arciumClusterOffset, offset),
    clusterAccount,
    mxeAccount: getMXEAccAddress(program.programId),
    mempoolAccount: getMempoolAccAddress(arciumEnv.arciumClusterOffset),
    executingPool: getExecutingPoolAccAddress(arciumEnv.arciumClusterOffset),
    compDefAccount: getCompDefAccAddress(
      program.programId,
      Buffer.from(getCompDefAccOffset(compDefName)).readUInt32LE()
    ),
  });

  it("flips a box and the decrypted result matches the transparent curve", async () => {
    // 1. bootstrap comp defs + upload circuits
    for (const name of ["init_curve", "init_treasury", "flip_box"]) {
      await initCompDef(name);
    }

    const mxePublicKey = await getMXEPublicKeyWithRetry(provider as anchor.AnchorProvider, program.programId);
    const sharedSecret = x25519.getSharedSecret(privateKey, mxePublicKey);
    const cipher = new RescueCipher(sharedSecret);

    // 2. initialize singletons
    await program.methods
      .initialize(new anchor.BN(K.toString()), Number(FEE_BPS), owner.publicKey)
      .accounts({ founder: owner.publicKey })
      .signers([owner])
      .rpc({ commitment: "confirmed" });

    // 3. seed encrypted curve (init_curve)
    {
      const offset = new anchor.BN(randomBytes(8), "hex");
      await program.methods
        .seedCurve(offset, new anchor.BN(GENESIS_R_SOL.toString()), new anchor.BN(GENESIS_R_TOK.toString()))
        .accountsPartial({ payer: owner.publicKey, config: configPda, curve: curvePda, ...arciumAccounts(offset, "init_curve") })
        .signers([owner])
        .rpc({ skipPreflight: true, commitment: "confirmed" });
      await awaitComputationFinalization(provider as anchor.AnchorProvider, offset, program.programId, "confirmed");
    }

    // 4. seed encrypted treasury (init_treasury)
    {
      const offset = new anchor.BN(randomBytes(8), "hex");
      await program.methods
        .seedTreasury(offset)
        .accountsPartial({ payer: owner.publicKey, config: configPda, treasury: treasuryPda, ...arciumAccounts(offset, "init_treasury") })
        .signers([owner])
        .rpc({ skipPreflight: true, commitment: "confirmed" });
      await awaitComputationFinalization(provider as anchor.AnchorProvider, offset, program.programId, "confirmed");
    }

    // 5. register a box with client-encrypted genesis state {sol=DEPOSIT, perp=0, in_perp=false, cost_basis=0}
    const boxNonce = randomBytes(16);
    const genesis = [DEPOSIT, 0n, 0n, 0n]; // sol, perp, in_perp(false), cost_basis
    const boxCt = cipher.encrypt(genesis, boxNonce);
    await program.methods
      .registerBox(
        boxCt.map((c) => Array.from(c)) as any,
        Array.from(publicKey),
        new anchor.BN(deserializeLE(boxNonce).toString())
      )
      .accounts({ owner: owner.publicKey, config: configPda })
      .signers([owner])
      .rpc({ commitment: "confirmed" });

    // 6. queue the confidential flip
    const flipOffset = new anchor.BN(randomBytes(8), "hex");
    await program.methods
      .queueFlip(flipOffset)
      .accountsPartial({
        payer: owner.publicKey,
        config: configPda,
        curve: curvePda,
        treasury: treasuryPda,
        playerBox: boxPda,
        ...arciumAccounts(flipOffset, "flip_box"),
      })
      .signers([owner])
      .rpc({ skipPreflight: true, commitment: "confirmed" });
    await awaitComputationFinalization(provider as anchor.AnchorProvider, flipOffset, program.programId, "confirmed");

    // 7. decrypt the box and compare to the transparent curve
    const boxAcct = await program.account.playerBox.fetch(boxPda);
    const newNonce = Uint8Array.from((boxAcct.nonce as anchor.BN).toArray("le", 16));
    const ctArr = (boxAcct.ct as number[][]).map((c) => Uint8Array.from(c));
    const [sol, perp, inPerp, costBasis] = cipher.decrypt(ctArr, newNonce);

    // Expected: in_perp false → BUY: fee on sol, buy_spend = DEPOSIT - fee
    const fee = (DEPOSIT * FEE_BPS) / 10_000n;
    const buySpend = DEPOSIT - fee;
    const expected = buy(GENESIS_R_SOL, GENESIS_R_TOK, K, buySpend);

    console.log("decrypted box:", { sol, perp, inPerp, costBasis });
    console.log("expected perp (transparent):", expected.tokOut);

    expect(inPerp).to.equal(1n);
    expect(sol).to.equal(0n);
    expect(costBasis).to.equal(buySpend);
    expect(perp).to.equal(expected.tokOut); // <-- M0a runtime gate: MPC == transparent curve, exactly
  });

  // ---- helpers ----
  async function initCompDef(name: string): Promise<void> {
    const baseSeed = getArciumAccountBaseSeed("ComputationDefinitionAccount");
    const offset = getCompDefAccOffset(name);
    const compDefPDA = PublicKey.findProgramAddressSync(
      [baseSeed, program.programId.toBuffer(), offset],
      getArciumProgramId()
    )[0];
    const mxeAccount = getMXEAccAddress(program.programId);
    const mxeAcc = await arciumProgram.account.mxeAccount.fetch(mxeAccount);
    const lutAddress = getLookupTableAddress(program.programId, mxeAcc.lutOffsetSlot);

    const method =
      name === "init_curve"
        ? program.methods.initCurveCompDef()
        : name === "init_treasury"
        ? program.methods.initTreasuryCompDef()
        : program.methods.initFlipBoxCompDef();

    await method
      .accounts({ compDefAccount: compDefPDA, payer: owner.publicKey, mxeAccount, addressLookupTable: lutAddress })
      .signers([owner])
      .rpc({ commitment: "confirmed" });

    const rawCircuit = fs.readFileSync(`build/${name}.arcis`);
    await uploadCircuit(provider as anchor.AnchorProvider, name, program.programId, rawCircuit, true, 500, {
      skipPreflight: true,
      preflightCommitment: "confirmed",
      commitment: "confirmed",
    });
  }
});

async function getMXEPublicKeyWithRetry(
  provider: anchor.AnchorProvider,
  programId: PublicKey,
  maxRetries = 20,
  retryDelayMs = 500
): Promise<Uint8Array> {
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const k = await getMXEPublicKey(provider, programId);
      if (k) return k;
    } catch (e) {
      /* retry */
    }
    if (attempt < maxRetries) await new Promise((r) => setTimeout(r, retryDelayMs));
  }
  throw new Error(`Failed to fetch MXE public key after ${maxRetries} attempts`);
}

function readKpJson(path: string): anchor.web3.Keypair {
  const file = fs.readFileSync(path);
  return anchor.web3.Keypair.fromSecretKey(new Uint8Array(JSON.parse(file.toString())));
}
