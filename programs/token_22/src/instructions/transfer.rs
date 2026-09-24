use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{spl_token_2022, TokenInterface};
use spl_token_2022::extension::{
    transfer_fee::{instruction as transfer_fee_instruction, TransferFeeConfig},
    BaseStateWithExtensions, StateWithExtensions,
};
use spl_token_2022::state::Mint as MintState;

use crate::errors::RemittanceError;

/// Task 2 — transfer using `transfer_checked_with_fee` (never plain
pub fn transfer_with_fee(ctx: Context<TransferWithFee>, amount: u64, decimals: u8) -> Result<()> {
    let expected_fee = {
        let data = ctx.accounts.mint.try_borrow_data()?;
        let state = StateWithExtensions::<MintState>::unpack(&data)?;
        let config = state
            .get_extension::<TransferFeeConfig>()
            .map_err(|_| error!(RemittanceError::MissingTransferFeeConfig))?;
        let epoch = Clock::get()?.epoch;
        u64::from(
            config
                .calculate_epoch_fee(epoch, amount)
                .ok_or(error!(RemittanceError::FeeCalculationFailed))?,
        )
    };

    invoke(
        &transfer_fee_instruction::transfer_checked_with_fee(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.source.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.destination.key(),
            &ctx.accounts.authority.key(),
            &[],
            amount,
            decimals,
            expected_fee,
        )?,
        &[
            ctx.accounts.source.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.destination.to_account_info(),
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;

    msg!("transferred {}, withheld fee {}", amount, expected_fee);
    Ok(())
}

#[derive(Accounts)]
pub struct TransferWithFee<'info> {
    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022; also read directly via
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    pub authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}
