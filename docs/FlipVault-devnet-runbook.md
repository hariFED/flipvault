# FlipVault — Devnet Runbook

All commands run **inside the dev container** (`./dev.ps1 shell`, or prefix with
`docker compose exec dev ...`). The Anchor workspace is at `/workspace/flipvault`.

Deploy wallet: **`J3A4bYr8PEmhGkpAyBgkupEXjpZUJCiYUVhX2hV42b7Y`**
(keypair at `/root/.config/solana/devnet.json`, persisted in the `solana-config` Docker volume —
do not `./dev.ps1 clean` until you're done, or it's wiped).

Program ID: `EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H`

## 0. Fund the wallet (you do this)
Send ~**6 devnet SOL** to the address above via <https://faucet.solana.com> (network = Devnet).
Budget: program rent ≈ 2.9 SOL · account rents + 1 SOL seed bankroll ≈ 1.5 SOL · ORAO fees + headroom.

Verify:
```bash
solana balance J3A4bYr8PEmhGkpAyBgkupEXjpZUJCiYUVhX2hV42b7Y --url devnet
```

## 1. Deploy the program
```bash
cd /workspace/flipvault
anchor deploy --provider.cluster devnet --provider.wallet /root/.config/solana/devnet.json
```

## 2. Run the client scripts
Each script reads the cluster + wallet from env:
```bash
export ANCHOR_PROVIDER_URL=https://api.devnet.solana.com
export ANCHOR_WALLET=/root/.config/solana/devnet.json
cd /workspace/flipvault

npx ts-node scripts/initialize.ts          # fund reserve, set curve/fee/round, create vaults
npx ts-node scripts/status.ts              # inspect config, balances, vaults
npx ts-node scripts/deposit.ts 0 0 200000000   # 0.2 SOL into vault0 SOL tranche
npx ts-node scripts/round.ts               # KEEPER: commit -> wait for VRF -> settle (one flip)
npx ts-node scripts/status.ts              # see which vault flipped
npx ts-node scripts/withdraw.ts 0 0 <shares>
npx ts-node scripts/sweep.ts <lamports>    # treasury authority pulls fees
npx ts-node scripts/recover.ts             # cancel a VRF-stuck round (after RECOVER_AFTER_SECS)
```
`initialize.ts` honors env overrides: `SEED_SOL`, `INIT_RTOK`, `ROUND_SECS`, `FEE_BPS`, `MIN_RESERVE`.

## 3. Verify the live round
After `scripts/round.ts`, confirm via `status.ts` that one vault's tranches swapped
(SOL↔TOKEN) and `selected_vault` updated. The reserve + Σ(SOL-tranche) sum must be unchanged
by the flip (only deposits/withdrawals move it).

## 4. Make it immutable (IRREVERSIBLE — only after you're satisfied)
```bash
solana program set-upgrade-authority EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H \
  --final --url devnet --keypair /root/.config/solana/devnet.json
```
After this, nobody can upgrade the program. The reserve/curve/vault funds were never
admin-controllable; only the treasury authority can sweep accrued fees.
