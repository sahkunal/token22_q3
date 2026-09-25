use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{
    initialize_mint2, mint_close_authority_initialize, transfer_fee_initialize, spl_token_2022,
    InitializeMint2, MintCloseAuthorityInitialize, TokenInterface, TransferFeeInitialize,
};
use spl_token_2022::{
    extension::{
        confidential_transfer::instruction as confidential_instruction,
        confidential_transfer_fee::instruction as confidential_fee_instruction,
        default_account_state::instruction as default_account_state_instruction,
        metadata_pointer::instruction as metadata_pointer_instruction,
        ExtensionType,
    },
    instruction as token_instruction,
    state::{AccountState, Mint as MintState},
};
use spl_token_metadata_interface::instruction as token_metadata_instruction;

use crate::constants::CONFIDENTIAL_EXTENSIONS;

/// Task 5 — re-issue the mint: carry the base four extensions forward, add

pub fn create_remittance_mint_v2(
    ctx: Context<CreateRemittanceMintV2>,
    decimals: u8,
    basis_points: u16,
    maximum_fee: u64,
    withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    let space = ExtensionType::try_calculate_account_len::<MintState>(CONFIDENTIAL_EXTENSIONS)?;
    let lamports = Rent::get()?.minimum_balance(space);

    anchor_lang::system_program::create_account(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.mint.to_account_info(),
            },
        ),
        lamports,
        space as u64,
        &ctx.accounts.token_program.key(),
    )?;

    let mint_info = ctx.accounts.mint.to_account_info();
    let program_info = ctx.accounts.token_program.to_account_info();
    let base_infos = [mint_info.clone(), program_info.clone()];

    mint_close_authority_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            MintCloseAuthorityInitialize {
                token_program_id: program_info.clone(),
                mint: mint_info.clone(),
            },
        ),
        Some(&ctx.accounts.payer.key()),
    )?;

    invoke(
        &metadata_pointer_instruction::initialize(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            Some(ctx.accounts.payer.key()),
            Some(ctx.accounts.mint.key()),
        )?,
        &base_infos,
    )?;

    transfer_fee_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferFeeInitialize {
                token_program_id: program_info.clone(),
                mint: mint_info.clone(),
            },
        ),
        Some(&ctx.accounts.payer.key()),
        Some(&ctx.accounts.payer.key()),
        basis_points,
        maximum_fee,
    )?;

    invoke(
        &default_account_state_instruction::initialize_default_account_state(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            &AccountState::Frozen,
        )?,
        &base_infos,
    )?;

    invoke(
        &token_instruction::initialize_permanent_delegate(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.payer.key(),
        )?,
        &base_infos,
    )?;

  
    invoke(
        &confidential_instruction::initialize_mint(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            Some(ctx.accounts.payer.key()),
            false,
            None,
        )?,
        &base_infos,
    )?;

    invoke(
        &confidential_fee_instruction::initialize_confidential_transfer_fee_config(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            Some(ctx.accounts.payer.key()),
            &withdraw_withheld_authority_elgamal_pubkey.into(),
        )?,
        &base_infos,
    )?;

    initialize_mint2(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            InitializeMint2 {
                mint: mint_info.clone(),
            },
        ),
        decimals,
        &ctx.accounts.payer.key(),
        Some(&ctx.accounts.payer.key()),
    )?;

    invoke(
        &token_metadata_instruction::initialize(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.payer.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.payer.key(),
            name,
            symbol,
            uri,
        ),
        &[mint_info.clone(), ctx.accounts.payer.to_account_info(), program_info],
    )?;

    msg!(
        "remittance mint v2 {} at {} bytes: seizable (public balance only) + confidential (manual approval)",
        ctx.accounts.mint.key(),
        space
    );
    Ok(())
}

#[derive(Accounts)]
pub struct CreateRemittanceMintV2<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: created and initialized in the handler.
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
