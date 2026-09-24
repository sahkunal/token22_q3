use anchor_lang::prelude::*;
use anchor_spl::token_interface::{spl_token_2022, TokenInterface};
use spl_token_2022::extension::{
    transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
};
use spl_token_2022::state::Mint as MintState;

use crate::constants::{BASE_EXTENSIONS, CONFIDENTIAL_EXTENSIONS};
use crate::errors::RemittanceError;

/// Task 3 — read mint state exclusively through `StateWithExtensions`,

pub fn assert_supported_mint(ctx: Context<AssertSupportedMint>) -> Result<()> {
    let account_info = ctx.accounts.mint.to_account_info();
    let data = account_info.try_borrow_data()?;
    let state = StateWithExtensions::<MintState>::unpack(&data)?;

    let decimals = state.base.decimals;

    for extension in state.get_extension_types()? {
        require!(
            BASE_EXTENSIONS.contains(&extension) || CONFIDENTIAL_EXTENSIONS.contains(&extension),
            RemittanceError::UnsupportedExtension
        );
    }

    let basis_points = match state.get_extension::<TransferFeeConfig>() {
        Ok(config) => u16::from(
            config
                .get_epoch_fee(Clock::get()?.epoch)
                .transfer_fee_basis_points,
        ),
        Err(_) => 0,
    };

    msg!("mint accepted: {} decimals, {} bps fee", decimals, basis_points);
    Ok(())
}

#[derive(Accounts)]
pub struct AssertSupportedMint<'info> {
    /// CHECK: ownership enforced below, contents allowlisted in the handler.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
