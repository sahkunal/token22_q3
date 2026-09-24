use anchor_lang::prelude::*;
use anchor_spl::token_interface::{transfer_checked, TokenInterface, TransferChecked};

pub fn permanent_delegate_seize(
    ctx: Context<PermanentDelegateSeize>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.permanent_delegate.to_account_info(),
            },
        ),
        amount,
        decimals,
    )?;
    msg!("seized {} from public balance without holder consent", amount);
    Ok(())
}

#[derive(Accounts)]
pub struct PermanentDelegateSeize<'info> {
    /// CHECK: validated by Token-2022. Note it is not a signer — that's the
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    pub permanent_delegate: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}
