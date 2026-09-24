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
