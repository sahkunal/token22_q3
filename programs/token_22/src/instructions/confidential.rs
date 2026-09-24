use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{spl_token_2022, TokenInterface};
use spl_token_2022::extension::{
    confidential_transfer::{
        instruction as confidential_instruction,
        ConfidentialTransferAccount,
        DecryptableBalance,
    },
    confidential_transfer::ProofLocation,
    BaseStateWithExtensions, StateWithExtensions,
};
use spl_token_2022::state::Account as TokenAccountState;

use crate::constants::{AE_CIPHERTEXT_LEN, EL_GAMAL_CIPHERTEXT_LEN};
use crate::errors::RemittanceError;
pub fn configure_confidential_account(
    ctx: Context<ConfigureConfidentialAccount>,
    decryptable_zero_balance: [u8; AE_CIPHERTEXT_LEN],
    maximum_pending_balance_credit_counter: u64,
) -> Result<()> {
    let balance = DecryptableBalance::from(decryptable_zero_balance);
    let ixs = confidential_instruction::configure_account(
        &ctx.accounts.token_program.key(),
        &ctx.accounts.token_account.key(),
        &ctx.accounts.mint.key(),
        &balance,
        maximum_pending_balance_credit_counter,
        &ctx.accounts.authority.key(),
        &[],
        ProofLocation::ContextStateAccount(&ctx.accounts.pubkey_validity_proof_context.key()),
    )?;
    let infos = [
        ctx.accounts.token_account.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.pubkey_validity_proof_context.to_account_info(),
    ];
    for ix in ixs.iter() {
        invoke(ix, &infos)?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct ConfigureConfidentialAccount<'info> {
    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,
    pub authority: Signer<'info>,

    /// CHECK: pre-staged zk-elgamal-proof-program context-state account
    pub pubkey_validity_proof_context: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}


pub fn deposit_confidential(ctx: Context<DepositConfidential>, amount: u64, decimals: u8) -> Result<()> {
    let ix = confidential_instruction::deposit(
        &ctx.accounts.token_program.key(),
        &ctx.accounts.token_account.key(),
        &ctx.accounts.mint.key(),
        amount,
        decimals,
        &ctx.accounts.authority.key(),
        &[],
    )?;
    invoke(
        &ix,
        &[
            ctx.accounts.token_account.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct DepositConfidential<'info> {
    /// CHECK: validated by Token-2022, which rejects any account that is
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022 during the deposit.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}


pub fn apply_pending_balance(
    ctx: Context<ApplyPendingBalance>,
    expected_pending_balance_credit_counter: u64,
    new_decryptable_available_balance: [u8; AE_CIPHERTEXT_LEN],
) -> Result<()> {
    let balance = DecryptableBalance::from(new_decryptable_available_balance);
    let ix = confidential_instruction::apply_pending_balance(
        &ctx.accounts.token_program.key(),
        &ctx.accounts.token_account.key(),
        expected_pending_balance_credit_counter,
        &balance,
        &ctx.accounts.authority.key(),
        &[],
    )?;
    invoke(
        &ix,
        &[
            ctx.accounts.token_account.to_account_info(),
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct ApplyPendingBalance<'info> {
    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    pub authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn confidential_transfer(
    ctx: Context<ConfidentialTransfer>,
    new_source_decryptable_available_balance: [u8; AE_CIPHERTEXT_LEN],
    ciphertext_lo: [u8; EL_GAMAL_CIPHERTEXT_LEN],
    ciphertext_hi: [u8; EL_GAMAL_CIPHERTEXT_LEN],
) -> Result<()> {
    let balance = DecryptableBalance::from(new_source_decryptable_available_balance);
    let ixs = confidential_instruction::transfer(
        &ctx.accounts.token_program.key(),
        &ctx.accounts.source.key(),
        &ctx.accounts.mint.key(),
        &ctx.accounts.destination.key(),
        &balance,
        &ciphertext_lo.into(),
        &ciphertext_hi.into(),
        &ctx.accounts.authority.key(),
        &[],
        ProofLocation::ContextStateAccount(&ctx.accounts.equality_proof_context.key()),
        ProofLocation::ContextStateAccount(&ctx.accounts.ciphertext_validity_proof_context.key()),
        ProofLocation::ContextStateAccount(&ctx.accounts.range_proof_context.key()),
    )?;
    let infos = [
        ctx.accounts.source.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.destination.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.equality_proof_context.to_account_info(),
        ctx.accounts.ciphertext_validity_proof_context.to_account_info(),
        ctx.accounts.range_proof_context.to_account_info(),
    ];
    for ix in ixs.iter() {
        invoke(ix, &infos)?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct ConfidentialTransfer<'info> {
    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    /// CHECK: pre-staged context-state account holding the
    /// ciphertext-commitment equality proof for this transfer.
    pub equality_proof_context: UncheckedAccount<'info>,

    /// CHECK: pre-staged context-state account holding the batched
    /// ciphertext-validity proof.
    pub ciphertext_validity_proof_context: UncheckedAccount<'info>,

    /// CHECK: pre-staged context-state account holding the batched range
    /// proof.
    pub range_proof_context: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn withdraw_confidential(
    ctx: Context<WithdrawConfidential>,
    amount: u64,
    decimals: u8,
    new_decryptable_available_balance: [u8; AE_CIPHERTEXT_LEN],
) -> Result<()> {
    {
        let data = ctx.accounts.token_account.try_borrow_data()?;
        let state = StateWithExtensions::<TokenAccountState>::unpack(&data)?;
        let ct_ext = state
            .get_extension::<ConfidentialTransferAccount>()
            .map_err(|_| error!(RemittanceError::MissingConfidentialExtension))?;
        require!(
            u64::from(ct_ext.pending_balance_credit_counter) == 0,
            RemittanceError::PendingBalanceNotApplied
        );
    }

    let balance = DecryptableBalance::from(new_decryptable_available_balance);
    let ixs = confidential_instruction::withdraw(
        &ctx.accounts.token_program.key(),
        &ctx.accounts.token_account.key(),
        &ctx.accounts.mint.key(),
        amount,
        decimals,
        &balance,
        &ctx.accounts.authority.key(),
        &[],
        ProofLocation::ContextStateAccount(&ctx.accounts.equality_proof_context.key()),
        ProofLocation::ContextStateAccount(&ctx.accounts.range_proof_context.key()),
    )?;
    let infos = [
        ctx.accounts.token_account.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.equality_proof_context.to_account_info(),
        ctx.accounts.range_proof_context.to_account_info(),
    ];
    for ix in ixs.iter() {
        invoke(ix, &infos)?;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawConfidential<'info> {
    /// CHECK: validated by Token-2022; also read directly via
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    /// CHECK: pre-staged context-state account holding the equality proof.
    pub equality_proof_context: UncheckedAccount<'info>,

    /// CHECK: pre-staged context-state account holding the range proof.
    pub range_proof_context: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
