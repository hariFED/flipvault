//! FlipVault Path-B — confidential per-player perpetual (Arcium MPC).
//!
//! Public spine (this program): box identities, the SOL custody vault, round/selection (M2),
//! and the curve constant `k` + `fee_bps`. Confidential core (Arcium MXE, see `encrypted-ixs/`):
//! the curve reserves, the treasury, and each box's balance/position/P&L — all encrypted.
//!
//! This file is the M1 genesis + flip slice: bootstrap the encrypted curve/treasury, register a
//! box with client-encrypted genesis state, and run a confidential `flip_box` whose result is
//! written back into the persisted ciphertext accounts. Custody (deposit/withdraw) and public VRF
//! selection layer on in M1-complete / M2 per docs/FlipVault-pathb-backend-blueprint.md.

use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use arcium_client::idl::arcium::types::CallbackAccount;

mod constants;
mod error;
mod state;

use constants::*;
use error::*;
use state::*;

const COMP_DEF_OFFSET_INIT_CURVE: u32 = comp_def_offset("init_curve");
const COMP_DEF_OFFSET_INIT_TREASURY: u32 = comp_def_offset("init_treasury");
const COMP_DEF_OFFSET_FLIP_BOX: u32 = comp_def_offset("flip_box");
const COMP_DEF_OFFSET_CREDIT_BOX: u32 = comp_def_offset("credit_box");
const COMP_DEF_OFFSET_DEBIT_BOX: u32 = comp_def_offset("debit_box");

declare_id!("BH5GgPvyxUYLFHFMZ77g4DrY6fNpfu2u5XBFDn8E8xyr");

#[arcium_program]
pub mod flipvault_pathb {
    use super::*;

    // ===================== computation-definition bootstrap =====================

    pub fn init_curve_comp_def(ctx: Context<InitCurveCompDef>) -> Result<()> {
        init_computation_def(ctx.accounts, None)?;
        Ok(())
    }
    pub fn init_treasury_comp_def(ctx: Context<InitTreasuryCompDef>) -> Result<()> {
        init_computation_def(ctx.accounts, None)?;
        Ok(())
    }
    pub fn init_flip_box_comp_def(ctx: Context<InitFlipBoxCompDef>) -> Result<()> {
        init_computation_def(ctx.accounts, None)?;
        Ok(())
    }

    // ===================== singleton init =====================

    pub fn initialize(
        ctx: Context<Initialize>,
        k: u128,
        fee_bps: u16,
        treasury_authority: Pubkey,
    ) -> Result<()> {
        require!(fee_bps <= MAX_FEE_BPS, PathBError::FeeTooHigh);
        require!(k > 0, PathBError::InvalidParams);
        let c = &mut ctx.accounts.config;
        c.bump = ctx.bumps.config;
        c.treasury_authority = treasury_authority;
        c.k = k;
        c.fee_bps = fee_bps;
        c.active_box_count = 0;
        c.curve_version = 0;
        c.curve_ready = false;
        c.treasury_ready = false;
        ctx.accounts.curve.bump = ctx.bumps.curve;
        ctx.accounts.treasury.bump = ctx.bumps.treasury;
        ctx.accounts.vault.bump = ctx.bumps.vault;
        Ok(())
    }

    // ===================== genesis: mint encrypted curve =====================

    pub fn seed_curve(
        ctx: Context<SeedCurve>,
        computation_offset: u64,
        r_sol: u128,
        r_tok: u128,
    ) -> Result<()> {
        ctx.accounts.curve.ct = [[0u8; 32]; CURVE_SCALARS];
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
        let args = ArgBuilder::new()
            .plaintext_u128(r_sol)
            .plaintext_u128(r_tok)
            .build();
        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![InitCurveCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[CallbackAccount {
                    pubkey: ctx.accounts.curve.key(),
                    is_writable: true,
                }],
            )?],
            1,
            0,
            0,
        )?;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "init_curve")]
    pub fn init_curve_callback(
        ctx: Context<InitCurveCallback>,
        output: SignedComputationOutputs<InitCurveOutput>,
    ) -> Result<()> {
        let o = match output.verify_output(
            &ctx.accounts.cluster_account,
            &ctx.accounts.computation_account,
        ) {
            Ok(InitCurveOutput { field_0 }) => field_0,
            Err(_) => return Err(PathBError::AbortedComputation.into()),
        };
        ctx.accounts.curve.ct = o.ciphertexts;
        ctx.accounts.curve.nonce = o.nonce;
        ctx.accounts.config.curve_ready = true;
        Ok(())
    }

    // ===================== genesis: mint encrypted treasury =====================

    pub fn seed_treasury(ctx: Context<SeedTreasury>, computation_offset: u64) -> Result<()> {
        ctx.accounts.treasury.ct = [[0u8; 32]; TREASURY_SCALARS];
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
        let args = ArgBuilder::new().build();
        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![InitTreasuryCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[CallbackAccount {
                    pubkey: ctx.accounts.treasury.key(),
                    is_writable: true,
                }],
            )?],
            1,
            0,
            0,
        )?;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "init_treasury")]
    pub fn init_treasury_callback(
        ctx: Context<InitTreasuryCallback>,
        output: SignedComputationOutputs<InitTreasuryOutput>,
    ) -> Result<()> {
        let o = match output.verify_output(
            &ctx.accounts.cluster_account,
            &ctx.accounts.computation_account,
        ) {
            Ok(InitTreasuryOutput { field_0 }) => field_0,
            Err(_) => return Err(PathBError::AbortedComputation.into()),
        };
        ctx.accounts.treasury.ct = o.ciphertexts;
        ctx.accounts.treasury.nonce = o.nonce;
        ctx.accounts.config.treasury_ready = true;
        Ok(())
    }

    // ===================== register a box (client genesis ciphertext) =====================

    pub fn register_box(
        ctx: Context<RegisterBox>,
        ct: [[u8; 32]; BOX_SCALARS],
        enc_pubkey: [u8; 32],
        nonce: u128,
    ) -> Result<()> {
        require!(
            ctx.accounts.config.active_box_count < MAX_BOXES,
            PathBError::RegistryFull
        );
        let idx = ctx.accounts.config.active_box_count;
        let b = &mut ctx.accounts.player_box;
        b.bump = ctx.bumps.player_box;
        b.ct = ct;
        b.enc_pubkey = enc_pubkey;
        b.nonce = nonce;
        b.owner = ctx.accounts.owner.key();
        b.index = idx;
        b.pending = false;
        b.curve_version_at_queue = 0;
        b.pending_withdraw = 0;
        ctx.accounts.config.active_box_count = idx + 1;
        Ok(())
    }

    // ===================== comp-def bootstrap for custody circuits =====================

    pub fn init_credit_box_comp_def(ctx: Context<InitCreditBoxCompDef>) -> Result<()> {
        init_computation_def(ctx.accounts, None)?;
        Ok(())
    }
    pub fn init_debit_box_comp_def(ctx: Context<InitDebitBoxCompDef>) -> Result<()> {
        init_computation_def(ctx.accounts, None)?;
        Ok(())
    }

    // ===================== deposit (public SOL in -> encrypted credit) =====================

    pub fn deposit(ctx: Context<Deposit>, computation_offset: u64, amount: u64) -> Result<()> {
        require!(!ctx.accounts.player_box.pending, PathBError::BoxPending);
        require!(amount > 0, PathBError::InvalidParams);

        // Move real SOL owner -> vault (amount is PUBLIC at the custody boundary, v1).
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.owner.key(),
                &ctx.accounts.vault.key(),
                amount,
            ),
            &[
                ctx.accounts.owner.to_account_info(),
                ctx.accounts.vault.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        let box_key = ctx.accounts.player_box.key();
        let box_nonce = ctx.accounts.player_box.nonce;
        let box_pubkey = ctx.accounts.player_box.enc_pubkey;
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        let args = ArgBuilder::new()
            .x25519_pubkey(box_pubkey)
            .plaintext_u128(box_nonce)
            .account(box_key, BOX_CT_OFFSET, BOX_CT_LEN)
            .plaintext_u128(amount as u128)
            .build();

        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![CreditBoxCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[CallbackAccount { pubkey: box_key, is_writable: true }],
            )?],
            1,
            0,
            0,
        )?;
        ctx.accounts.player_box.pending = true;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "credit_box")]
    pub fn credit_box_callback(
        ctx: Context<CreditBoxCallback>,
        output: SignedComputationOutputs<CreditBoxOutput>,
    ) -> Result<()> {
        let box_out = match output.verify_output(
            &ctx.accounts.cluster_account,
            &ctx.accounts.computation_account,
        ) {
            Ok(CreditBoxOutput { field_0 }) => field_0,
            Err(_) => return Err(PathBError::AbortedComputation.into()),
        };
        ctx.accounts.player_box.ct = box_out.ciphertexts;
        ctx.accounts.player_box.nonce = box_out.nonce;
        ctx.accounts.player_box.enc_pubkey = box_out.encryption_key;
        ctx.accounts.player_box.pending = false;
        Ok(())
    }

    // ===================== withdraw (encrypted debit -> public SOL out) =====================

    pub fn withdraw(ctx: Context<Withdraw>, computation_offset: u64, amount: u64) -> Result<()> {
        require!(!ctx.accounts.player_box.pending, PathBError::BoxPending);
        require!(amount > 0, PathBError::InvalidParams);

        let box_key = ctx.accounts.player_box.key();
        let box_nonce = ctx.accounts.player_box.nonce;
        let box_pubkey = ctx.accounts.player_box.enc_pubkey;
        let vault_key = ctx.accounts.vault.key();
        let owner_key = ctx.accounts.owner.key();
        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        let args = ArgBuilder::new()
            .x25519_pubkey(box_pubkey)
            .plaintext_u128(box_nonce)
            .account(box_key, BOX_CT_OFFSET, BOX_CT_LEN)
            .plaintext_u128(amount as u128)
            .build();

        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![DebitBoxCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[
                    CallbackAccount { pubkey: box_key, is_writable: true },
                    CallbackAccount { pubkey: vault_key, is_writable: true },
                    CallbackAccount { pubkey: owner_key, is_writable: true },
                ],
            )?],
            1,
            0,
            0,
        )?;
        // remember the payout for the callback (callbacks receive no instruction args)
        ctx.accounts.player_box.pending = true;
        ctx.accounts.player_box.pending_withdraw = amount;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "debit_box")]
    pub fn debit_box_callback(
        ctx: Context<DebitBoxCallback>,
        output: SignedComputationOutputs<DebitBoxOutput>,
    ) -> Result<()> {
        let (box_out, ok) = match output.verify_output(
            &ctx.accounts.cluster_account,
            &ctx.accounts.computation_account,
        ) {
            Ok(DebitBoxOutput {
                field_0: DebitBoxOutputStruct0 { field_0, field_1 },
            }) => (field_0, field_1),
            Err(_) => return Err(PathBError::AbortedComputation.into()),
        };
        ctx.accounts.player_box.ct = box_out.ciphertexts;
        ctx.accounts.player_box.nonce = box_out.nonce;
        ctx.accounts.player_box.enc_pubkey = box_out.encryption_key;

        let amount = ctx.accounts.player_box.pending_withdraw;
        if ok && amount > 0 {
            // direct lamport mutation: vault (program-owned) -> owner
            **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? -= amount;
            **ctx.accounts.owner.try_borrow_mut_lamports()? += amount;
        }
        ctx.accounts.player_box.pending_withdraw = 0;
        ctx.accounts.player_box.pending = false;
        Ok(())
    }

    // ===================== confidential flip =====================

    pub fn queue_flip(ctx: Context<QueueFlip>, computation_offset: u64) -> Result<()> {
        require!(
            ctx.accounts.config.curve_ready && ctx.accounts.config.treasury_ready,
            PathBError::InvalidParams
        );
        require!(!ctx.accounts.player_box.pending, PathBError::BoxPending);

        let k = ctx.accounts.config.k;
        let fee_bps = ctx.accounts.config.fee_bps as u128;
        let curve_key = ctx.accounts.curve.key();
        let curve_nonce = ctx.accounts.curve.nonce;
        let treasury_key = ctx.accounts.treasury.key();
        let treasury_nonce = ctx.accounts.treasury.nonce;
        let box_key = ctx.accounts.player_box.key();
        let box_nonce = ctx.accounts.player_box.nonce;
        let box_pubkey = ctx.accounts.player_box.enc_pubkey;

        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

        let args = ArgBuilder::new()
            // Enc<Mxe, Curve>
            .plaintext_u128(curve_nonce)
            .account(curve_key, CURVE_CT_OFFSET, CURVE_CT_LEN)
            // Enc<Mxe, u128 treasury>
            .plaintext_u128(treasury_nonce)
            .account(treasury_key, TREASURY_CT_OFFSET, TREASURY_CT_LEN)
            // Enc<Shared, BoxState>
            .x25519_pubkey(box_pubkey)
            .plaintext_u128(box_nonce)
            .account(box_key, BOX_CT_OFFSET, BOX_CT_LEN)
            // public scalars
            .plaintext_u128(k)
            .plaintext_u128(fee_bps)
            .build();

        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![FlipBoxCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[
                    CallbackAccount { pubkey: curve_key, is_writable: true },
                    CallbackAccount { pubkey: treasury_key, is_writable: true },
                    CallbackAccount { pubkey: box_key, is_writable: true },
                ],
            )?],
            1,
            0,
            0,
        )?;

        // lock the box + snapshot the curve version for the stale-callback guard
        let v = ctx.accounts.config.curve_version;
        let b = &mut ctx.accounts.player_box;
        b.pending = true;
        b.curve_version_at_queue = v;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "flip_box")]
    pub fn flip_box_callback(
        ctx: Context<FlipBoxCallback>,
        output: SignedComputationOutputs<FlipBoxOutput>,
    ) -> Result<()> {
        let (curve_out, treasury_out, box_out) = match output.verify_output(
            &ctx.accounts.cluster_account,
            &ctx.accounts.computation_account,
        ) {
            Ok(FlipBoxOutput {
                field_0:
                    FlipBoxOutputStruct0 {
                        field_0,
                        field_1,
                        field_2,
                    },
            }) => (field_0, field_1, field_2),
            Err(_) => return Err(PathBError::AbortedComputation.into()),
        };

        // stale-callback guard: reject if a newer committed flip advanced the curve since queue
        require!(
            ctx.accounts.config.curve_version == ctx.accounts.player_box.curve_version_at_queue,
            PathBError::StaleCallback
        );

        ctx.accounts.curve.ct = curve_out.ciphertexts;
        ctx.accounts.curve.nonce = curve_out.nonce;
        ctx.accounts.treasury.ct = treasury_out.ciphertexts;
        ctx.accounts.treasury.nonce = treasury_out.nonce;
        ctx.accounts.player_box.ct = box_out.ciphertexts;
        ctx.accounts.player_box.nonce = box_out.nonce;
        ctx.accounts.player_box.enc_pubkey = box_out.encryption_key;

        ctx.accounts.config.curve_version += 1;
        ctx.accounts.player_box.pending = false;
        emit!(FlipSettled {
            box_key: ctx.accounts.player_box.key(),
        });
        Ok(())
    }
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct FlipSettled {
    pub box_key: Pubkey,
}

// ============================================================================
// comp-def init contexts (one per circuit; identical shape)
// ============================================================================

#[init_computation_definition_accounts("init_curve", payer)]
#[derive(Accounts)]
pub struct InitCurveCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account, checked by arcium program (not initialized yet).
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table, checked by arcium program.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("init_treasury", payer)]
#[derive(Accounts)]
pub struct InitTreasuryCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("flip_box", payer)]
#[derive(Accounts)]
pub struct InitFlipBoxCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

// ============================================================================
// initialize
// ============================================================================

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub founder: Signer<'info>,
    #[account(
        init,
        payer = founder,
        space = 8 + PathBConfig::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Box<Account<'info, PathBConfig>>,
    #[account(
        init,
        payer = founder,
        space = 8 + CurveState::INIT_SPACE,
        seeds = [CURVE_SEED],
        bump
    )]
    pub curve: Box<Account<'info, CurveState>>,
    #[account(
        init,
        payer = founder,
        space = 8 + TreasuryState::INIT_SPACE,
        seeds = [TREASURY_SEED],
        bump
    )]
    pub treasury: Box<Account<'info, TreasuryState>>,
    #[account(
        init,
        payer = founder,
        space = 8 + Vault::INIT_SPACE,
        seeds = [VAULT_SEED],
        bump
    )]
    pub vault: Box<Account<'info, Vault>>,
    pub system_program: Program<'info, System>,
}

// ============================================================================
// seed_curve (queue init_curve) + callback
// ============================================================================

#[queue_computation_accounts("init_curve", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct SeedCurve<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(init_if_needed, space = 9, payer = payer, seeds = [&SIGN_PDA_SEED], bump, address = derive_sign_pda!())]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: exec pool.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_CURVE))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // custom
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
    #[account(mut, seeds = [CURVE_SEED], bump = curve.bump)]
    pub curve: Box<Account<'info, CurveState>>,
}

#[callback_accounts("init_curve")]
#[derive(Accounts)]
pub struct InitCurveCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_CURVE))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: validated by the Arcium program; verify_output reads from it.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions sysvar.
    pub instructions_sysvar: UncheckedAccount<'info>,
    // custom (order MUST match the callback_ix custom-account slice)
    #[account(mut, seeds = [CURVE_SEED], bump = curve.bump)]
    pub curve: Box<Account<'info, CurveState>>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
}

// ============================================================================
// seed_treasury (queue init_treasury) + callback
// ============================================================================

#[queue_computation_accounts("init_treasury", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct SeedTreasury<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(init_if_needed, space = 9, payer = payer, seeds = [&SIGN_PDA_SEED], bump, address = derive_sign_pda!())]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: exec pool.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_TREASURY))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
    #[account(mut, seeds = [TREASURY_SEED], bump = treasury.bump)]
    pub treasury: Box<Account<'info, TreasuryState>>,
}

#[callback_accounts("init_treasury")]
#[derive(Accounts)]
pub struct InitTreasuryCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_INIT_TREASURY))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: validated by the Arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions sysvar.
    pub instructions_sysvar: UncheckedAccount<'info>,
    #[account(mut, seeds = [TREASURY_SEED], bump = treasury.bump)]
    pub treasury: Box<Account<'info, TreasuryState>>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
}

// ============================================================================
// register_box
// ============================================================================

#[derive(Accounts)]
pub struct RegisterBox<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
    #[account(
        init,
        payer = owner,
        space = 8 + PlayerBox::INIT_SPACE,
        seeds = [BOX_SEED, owner.key().as_ref()],
        bump
    )]
    pub player_box: Box<Account<'info, PlayerBox>>,
    pub system_program: Program<'info, System>,
}

// ============================================================================
// queue_flip + flip_box_callback
// ============================================================================

#[queue_computation_accounts("flip_box", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct QueueFlip<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(init_if_needed, space = 9, payer = payer, seeds = [&SIGN_PDA_SEED], bump, address = derive_sign_pda!())]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: exec pool.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_FLIP_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // custom
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
    #[account(seeds = [CURVE_SEED], bump = curve.bump)]
    pub curve: Box<Account<'info, CurveState>>,
    #[account(seeds = [TREASURY_SEED], bump = treasury.bump)]
    pub treasury: Box<Account<'info, TreasuryState>>,
    #[account(mut, seeds = [BOX_SEED, player_box.owner.as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
}

#[callback_accounts("flip_box")]
#[derive(Accounts)]
pub struct FlipBoxCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_FLIP_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: validated by the Arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions sysvar.
    pub instructions_sysvar: UncheckedAccount<'info>,
    // custom (order MUST match the callback_ix slice: curve, treasury, box)
    #[account(mut, seeds = [CURVE_SEED], bump = curve.bump)]
    pub curve: Box<Account<'info, CurveState>>,
    #[account(mut, seeds = [TREASURY_SEED], bump = treasury.bump)]
    pub treasury: Box<Account<'info, TreasuryState>>,
    #[account(mut, seeds = [BOX_SEED, player_box.owner.as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PathBConfig>>,
}

// ============================================================================
// custody comp-def init contexts
// ============================================================================

#[init_computation_definition_accounts("credit_box", payer)]
#[derive(Accounts)]
pub struct InitCreditBoxCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[init_computation_definition_accounts("debit_box", payer)]
#[derive(Accounts)]
pub struct InitDebitBoxCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot))]
    /// CHECK: address_lookup_table.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

// ============================================================================
// deposit (queue credit_box) + callback
// ============================================================================

#[queue_computation_accounts("credit_box", owner)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(init_if_needed, space = 9, payer = owner, seeds = [&SIGN_PDA_SEED], bump, address = derive_sign_pda!())]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: exec pool.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_CREDIT_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // custom
    #[account(mut, seeds = [VAULT_SEED], bump = vault.bump)]
    pub vault: Box<Account<'info, Vault>>,
    #[account(mut, seeds = [BOX_SEED, owner.key().as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
}

#[callback_accounts("credit_box")]
#[derive(Accounts)]
pub struct CreditBoxCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_CREDIT_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: validated by the Arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions sysvar.
    pub instructions_sysvar: UncheckedAccount<'info>,
    #[account(mut, seeds = [BOX_SEED, player_box.owner.as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
}

// ============================================================================
// withdraw (queue debit_box) + callback
// ============================================================================

#[queue_computation_accounts("debit_box", owner)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(init_if_needed, space = 9, payer = owner, seeds = [&SIGN_PDA_SEED], bump, address = derive_sign_pda!())]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut, address = derive_mempool_pda!(mxe_account))]
    /// CHECK: mempool.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(mut, address = derive_execpool_pda!(mxe_account))]
    /// CHECK: exec pool.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(mut, address = derive_comp_pda!(computation_offset, mxe_account))]
    /// CHECK: computation.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_DEBIT_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Box<Account<'info, Cluster>>,
    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,
    #[account(mut, address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,
    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
    // custom
    #[account(mut, seeds = [VAULT_SEED], bump = vault.bump)]
    pub vault: Box<Account<'info, Vault>>,
    #[account(mut, seeds = [BOX_SEED, owner.key().as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
}

#[callback_accounts("debit_box")]
#[derive(Accounts)]
pub struct DebitBoxCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_DEBIT_BOX))]
    pub comp_def_account: Box<Account<'info, ComputationDefinitionAccount>>,
    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    /// CHECK: validated by the Arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,
    #[account(address = ::arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: instructions sysvar.
    pub instructions_sysvar: UncheckedAccount<'info>,
    // custom (order MUST match the callback_ix slice: box, vault, owner)
    #[account(mut, seeds = [BOX_SEED, player_box.owner.as_ref()], bump = player_box.bump)]
    pub player_box: Box<Account<'info, PlayerBox>>,
    #[account(mut, seeds = [VAULT_SEED], bump = vault.bump)]
    pub vault: Box<Account<'info, Vault>>,
    #[account(mut, address = player_box.owner)]
    /// CHECK: the box owner; receives the withdrawal payout.
    pub owner: UncheckedAccount<'info>,
}
