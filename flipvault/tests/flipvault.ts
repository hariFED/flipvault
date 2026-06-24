import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Flipvault } from "../target/types/flipvault";
import {
  PublicKey,
  Keypair,
  SystemProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import { assert } from "chai";

describe("flipvault — core (VRF-independent)", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.flipvault as Program<Flipvault>;
  const connection = provider.connection;

  // Genesis params.
  const SEED_SOL = 10 * LAMPORTS_PER_SOL;
  const INIT_RTOK = 10 * LAMPORTS_PER_SOL;
  const ROUND_SECS = 30;
  const FEE_BPS = 1000; // 10%
  const MIN_RESERVE = 1_000_000; // 0.001 SOL
  const BPS_DENOM = 10_000;

  const user1 = Keypair.generate();
  const user2 = Keypair.generate();
  const feeRecipient = Keypair.generate();

  // PDAs.
  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);
  const [reservePda] = PublicKey.findProgramAddressSync([Buffer.from("reserve")], program.programId);
  const [treasuryPda] = PublicKey.findProgramAddressSync([Buffer.from("treasury")], program.programId);
  const vaultPda = (id: number) =>
    PublicKey.findProgramAddressSync([Buffer.from("vault"), Buffer.from([id])], program.programId)[0];
  const positionPda = (owner: PublicKey, vaultId: number, slot: number) =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("position"), owner.toBuffer(), Buffer.from([vaultId]), Buffer.from([slot])],
      program.programId
    )[0];

  const airdrop = async (pk: PublicKey, sol: number) => {
    const sig = await connection.requestAirdrop(pk, sol * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(sig, "confirmed");
  };
  const bal = (pk: PublicKey) => connection.getBalance(pk);
  const isSol = (tr: any) => tr.asset.sol !== undefined;

  // Q = reserve lamports + sum of SOL-tranche amounts (state). Flips would leave it constant;
  // deposits raise it, withdrawals lower it. No flips happen here, so we check exact deltas.
  const computeQ = async (): Promise<number> => {
    let q = await bal(reservePda);
    for (let i = 0; i < 4; i++) {
      const v = await program.account.vault.fetch(vaultPda(i));
      for (const tr of v.tranches) if (isSol(tr)) q += tr.amount.toNumber();
    }
    return q;
  };

  const expectErr = async (p: Promise<any>, label: string) => {
    try {
      await p;
      assert.fail(`expected error: ${label}`);
    } catch (e: any) {
      if (e?.message?.startsWith("expected error")) throw e;
    }
  };

  before(async () => {
    await airdrop(provider.wallet.publicKey, 50);
    await airdrop(user1.publicKey, 30);
    await airdrop(user2.publicKey, 30);
  });

  it("initializes the curve, reserve, treasury, and 4 vaults", async () => {
    await program.methods
      .initialize(
        new BN(SEED_SOL),
        new BN(INIT_RTOK),
        new BN(ROUND_SECS),
        FEE_BPS,
        new BN(MIN_RESERVE),
        feeRecipient.publicKey
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

    const cfg = await program.account.config.fetch(configPda);
    assert.equal(cfg.rTok.toString(), INIT_RTOK.toString());
    assert.equal(cfg.k.toString(), (BigInt(SEED_SOL) * BigInt(INIT_RTOK)).toString());
    assert.equal(cfg.feeBps, FEE_BPS);
    assert.equal(cfg.treasuryAuthority.toBase58(), feeRecipient.publicKey.toBase58());
    assert.isDefined(cfg.phase.idle);

    // Reserve holds seed_sol above its rent floor.
    const rentFloor = await connection.getMinimumBalanceForRentExemption(9);
    assert.equal((await bal(reservePda)) - rentFloor, SEED_SOL);

    // Genesis layout: slot 0 SOL, slot 1 TOKEN, all empty.
    const v0 = await program.account.vault.fetch(vaultPda(0));
    assert.isTrue(isSol(v0.tranches[0]));
    assert.isFalse(isSol(v0.tranches[1]));
    assert.equal(v0.tranches[0].amount.toNumber(), 0);
  });

  it("rejects deposit into a TOKEN tranche and below the minimum", async () => {
    await expectErr(
      program.methods
        .deposit(0, 1, new BN(LAMPORTS_PER_SOL)) // slot 1 = TOKEN
        .accountsPartial({
          user: user1.publicKey,
          config: configPda,
          vault: vaultPda(0),
          position: positionPda(user1.publicKey, 0, 1),
          systemProgram: SystemProgram.programId,
        })
        .signers([user1])
        .rpc(),
      "deposit into TOKEN tranche"
    );

    await expectErr(
      program.methods
        .deposit(0, 0, new BN(100)) // below MIN_DEPOSIT (1000)
        .accountsPartial({
          user: user1.publicKey,
          config: configPda,
          vault: vaultPda(0),
          position: positionPda(user1.publicKey, 0, 0),
          systemProgram: SystemProgram.programId,
        })
        .signers([user1])
        .rpc(),
      "dust deposit"
    );
  });

  it("first deposit mints shares 1:1 and raises Q by the deposit", async () => {
    const qBefore = await computeQ();
    const amount = 2 * LAMPORTS_PER_SOL;

    await program.methods
      .deposit(0, 0, new BN(amount))
      .accountsPartial({
        user: user1.publicKey,
        config: configPda,
        vault: vaultPda(0),
        position: positionPda(user1.publicKey, 0, 0),
        systemProgram: SystemProgram.programId,
      })
      .signers([user1])
      .rpc();

    const pos = await program.account.position.fetch(positionPda(user1.publicKey, 0, 0));
    const v0 = await program.account.vault.fetch(vaultPda(0));
    assert.equal(pos.shares.toNumber(), amount, "1:1 shares");
    assert.equal(v0.tranches[0].amount.toNumber(), amount);
    assert.equal(v0.tranches[0].totalShares.toNumber(), amount);
    assert.equal((await computeQ()) - qBefore, amount, "Q rises by deposit");
  });

  it("second depositor gets pro-rata shares", async () => {
    const amount = 1 * LAMPORTS_PER_SOL;
    const v0Before = await program.account.vault.fetch(vaultPda(0));
    const expectedShares = Math.floor(
      (amount * v0Before.tranches[0].totalShares.toNumber()) / v0Before.tranches[0].amount.toNumber()
    );

    await program.methods
      .deposit(0, 0, new BN(amount))
      .accountsPartial({
        user: user2.publicKey,
        config: configPda,
        vault: vaultPda(0),
        position: positionPda(user2.publicKey, 0, 0),
        systemProgram: SystemProgram.programId,
      })
      .signers([user2])
      .rpc();

    const pos = await program.account.position.fetch(positionPda(user2.publicKey, 0, 0));
    assert.equal(pos.shares.toNumber(), expectedShares);
  });

  it("is immune to the donation/inflation attack (uses in-state amount, not balance)", async () => {
    // user2 opens vault1 with 1 SOL (1:1).
    const open = 1 * LAMPORTS_PER_SOL;
    await program.methods
      .deposit(1, 0, new BN(open))
      .accountsPartial({
        user: user2.publicKey,
        config: configPda,
        vault: vaultPda(1),
        position: positionPda(user2.publicKey, 1, 0),
        systemProgram: SystemProgram.programId,
      })
      .signers([user2])
      .rpc();

    // Attacker donates 5 SOL straight to the vault PDA, trying to inflate share price.
    const donateTx = new anchor.web3.Transaction().add(
      SystemProgram.transfer({
        fromPubkey: user1.publicKey,
        toPubkey: vaultPda(1),
        lamports: 5 * LAMPORTS_PER_SOL,
      })
    );
    await provider.sendAndConfirm(donateTx, [user1]);

    // user1 deposits 1 SOL. If pricing used the (inflated) balance, shares would round down
    // far below 1e9; using in-state amount, shares == 1e9 exactly.
    await program.methods
      .deposit(1, 0, new BN(open))
      .accountsPartial({
        user: user1.publicKey,
        config: configPda,
        vault: vaultPda(1),
        position: positionPda(user1.publicKey, 1, 0),
        systemProgram: SystemProgram.programId,
      })
      .signers([user1])
      .rpc();

    const pos = await program.account.position.fetch(positionPda(user1.publicKey, 1, 0));
    assert.equal(pos.shares.toNumber(), open, "donation must not change share price");
  });

  it("withdraw pays 90% to the user and 10% to the treasury; Q drops by the gross", async () => {
    const shares = 1 * LAMPORTS_PER_SOL;
    const v0 = await program.account.vault.fetch(vaultPda(0));
    const payoutGross = Math.floor(
      (shares * v0.tranches[0].amount.toNumber()) / v0.tranches[0].totalShares.toNumber()
    );
    const fee = Math.floor((payoutGross * FEE_BPS) / BPS_DENOM);

    const treBefore = await bal(treasuryPda);
    const vaultBefore = await bal(vaultPda(0));
    const qBefore = await computeQ();

    await program.methods
      .withdraw(0, 0, new BN(shares))
      .accountsPartial({
        user: user1.publicKey,
        config: configPda,
        vault: vaultPda(0),
        position: positionPda(user1.publicKey, 0, 0),
        treasury: treasuryPda,
      })
      .signers([user1])
      .rpc();

    assert.equal((await bal(treasuryPda)) - treBefore, fee, "treasury gets 10%");
    assert.equal(vaultBefore - (await bal(vaultPda(0))), payoutGross, "vault drops by gross");
    assert.equal(qBefore - (await computeQ()), payoutGross, "Q drops by gross");
  });

  it("rejects withdraw from a TOKEN tranche", async () => {
    await expectErr(
      program.methods
        .withdraw(0, 1, new BN(1))
        .accountsPartial({
          user: user1.publicKey,
          config: configPda,
          vault: vaultPda(0),
          position: positionPda(user1.publicKey, 0, 1),
          treasury: treasuryPda,
        })
        .signers([user1])
        .rpc(),
      "withdraw from TOKEN tranche"
    );
  });

  it("lets the treasury authority sweep fees, and rejects others", async () => {
    const amount = await bal(treasuryPda);
    const rentFloor = await connection.getMinimumBalanceForRentExemption(9);
    const sweepable = amount - rentFloor;

    // Non-authority cannot sweep.
    await expectErr(
      program.methods
        .sweepTreasury(new BN(sweepable))
        .accountsPartial({
          authority: user1.publicKey,
          config: configPda,
          treasury: treasuryPda,
          recipient: user1.publicKey,
        })
        .signers([user1])
        .rpc(),
      "non-authority sweep"
    );

    // Authority sweeps to the fee recipient.
    const recvBefore = await bal(feeRecipient.publicKey);
    await airdrop(feeRecipient.publicKey, 1); // fund authority to pay tx fee
    await program.methods
      .sweepTreasury(new BN(sweepable))
      .accountsPartial({
        authority: feeRecipient.publicKey,
        config: configPda,
        treasury: treasuryPda,
        recipient: feeRecipient.publicKey,
      })
      .signers([feeRecipient])
      .rpc();

    assert.isAbove((await bal(feeRecipient.publicKey)) - recvBefore, 0);
  });
});
