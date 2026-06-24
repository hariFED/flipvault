use crate::error::FlipError;
use anchor_lang::prelude::*;

/// Move `amount` lamports out of a program-owned, data-carrying PDA (`from`) into `to` by
/// direct lamport mutation, keeping `from` at or above `from_rent_floor`. The System
/// Program cannot debit such accounts, so this is the required mechanism for the reserve,
/// the vaults, and the treasury. Crediting (`to`) is always allowed.
pub fn move_lamports_out<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    amount: u64,
    from_rent_floor: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let from_balance = from.lamports();
    let remaining = from_balance.checked_sub(amount).ok_or(FlipError::Overflow)?;
    require!(remaining >= from_rent_floor, FlipError::ReserveFloor);

    **from.try_borrow_mut_lamports()? = remaining;
    **to.try_borrow_mut_lamports()? = to
        .lamports()
        .checked_add(amount)
        .ok_or(FlipError::Overflow)?;
    Ok(())
}

/// Rent-exempt minimum for an account holding `data_len` bytes.
pub fn rent_floor(data_len: usize) -> Result<u64> {
    Ok(Rent::get()?.minimum_balance(data_len))
}

/// `floor(a * b / c)` computed in u128 and narrowed to u64. Errors on div-by-zero/overflow.
pub fn mul_div_floor(a: u64, b: u64, c: u64) -> Result<u64> {
    require!(c != 0, FlipError::DivByZero);
    let prod = (a as u128)
        .checked_mul(b as u128)
        .ok_or(FlipError::Overflow)?;
    let res = prod / (c as u128);
    u64::try_from(res).map_err(|_| error!(FlipError::Overflow))
}
