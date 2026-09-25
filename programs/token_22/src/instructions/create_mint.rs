use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{
    initialize_mint2, mint_close_authority_initialize, transfer_fee_initialize, spl_token_2022,
    InitializeMint2, MintCloseAuthorityInitialize, TokenInterface, TransferFeeInitialize,
};
use spl_token_2022::{
    extension::{
        default_account_state::instruction as default_account_state_instruction,
        metadata_pointer::instruction as metadata_pointer_instruction, ExtensionType,
    },
    state::{AccountState, Mint as MintState},
};

use crate::constants::BASE_EXTENSIONS;

/// Task 1 — mint stacking TransferFeeConfig, MetadataPointer (pointed at
/// the mint itself), DefaultAccountState(Frozen), and MintCloseAuthority,
/// sized via `ExtensionType::try_calculate_account_len`, with every
/// extension-init instruction ordered before InitializeMint.
pub fn create_remittance_mint(
    ctx: Context<CreateRemittanceMint>,
    decimals: u8,
    basis_points: u16,
    maximum_fee: u64,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    let space = ExtensionType::try_calculate_account_len::<MintState>(BASE_EXTENSIONS)?;
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

    // --- every extension-init instruction, in order, BEFORE InitializeMint2 ---

    mint_close_authority_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            MintCloseAuthorityInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        Some(&ctx.accounts.payer.key()),
    )?;

    // MetadataPointer points at the MINT ITSELF, not an off-chain/third
    // party account — this is what lets a wallet trust the metadata without
    // a registry lookup.
    invoke(
        &metadata_pointer_instruction::initialize(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            Some(ctx.accounts.payer.key()),
            Some(ctx.accounts.mint.key()),
        )?,
        &[
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;

    transfer_fee_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferFeeInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        Some(&ctx.accounts.payer.key()),
        Some(&ctx.accounts.payer.key()),
        basis_points,
        maximum_fee,
    )?;

    // New accounts for this mint start FROZEN until KYC clears — pairs with
    // the thaw instruction in `instructions::kyc`.
    invoke(
        &default_account_state_instruction::initialize_default_account_state(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            &AccountState::Frozen,
        )?,
        &[
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;

    initialize_mint2(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            InitializeMint2 {
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        decimals,
        &ctx.accounts.payer.key(),
        Some(&ctx.accounts.payer.key()),
    )?;

    invoke(
        &spl_token_metadata_interface::instruction::initialize(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.payer.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.payer.key(),
            name,
            symbol,
            uri,
        ),
        &[
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
      let required_lamports = Rent::get()?.minimum_balance(ctx.accounts.mint.data_len());
    let current_lamports = ctx.accounts.mint.lamports();
    if required_lamports > current_lamports {
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.mint.to_account_info(),
                },
            ),
            required_lamports - current_lamports,
        )?;
    }


    msg!(
        "remittance mint {} created, {} bytes, {} bps fee, frozen-by-default",
        ctx.accounts.mint.key(),
        space,
        basis_points
    );
    Ok(())
}

#[derive(Accounts)]
pub struct CreateRemittanceMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: created and initialized in the handler, required to sign
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
