use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{spl_token_2022, TokenInterface};
pub fn thaw_after_kyc(ctx: Context<ThawAfterKyc>) -> Result<()> {
    invoke(
        &spl_token_2022::instruction::thaw_account(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.token_account.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.freeze_authority.key(),
            &[],
        )?,
        &[
            ctx.accounts.token_account.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.freeze_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
    msg!(
        "account {} thawed post-KYC by {}",
        ctx.accounts.token_account.key(),
        ctx.accounts.freeze_authority.key()
    );
    Ok(())
}

#[derive(Accounts)]
pub struct ThawAfterKyc<'info> {
    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub freeze_authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}
