use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;

use instructions::*;

declare_id!("5yH9fhML84aDANqnxkTb8npLF6mpGhXnv1Qemom69L1d");

#[program]
pub mod token_22 {
    use super::*;

    /// Task 1 — mint stacking TransferFeeConfig, MetadataPointer (pointed
    /// at the mint itself), DefaultAccountState(Frozen), and
    /// MintCloseAuthority. See `instructions::create_mint`.
    pub fn create_remittance_mint(
        ctx: Context<CreateRemittanceMint>,
        decimals: u8,
        basis_points: u16,
        maximum_fee: u64,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_mint::create_remittance_mint(
            ctx, decimals, basis_points, maximum_fee, name, symbol, uri,
        )
    }

    /// Task 2 — transfer_checked_with_fee with the fee computed from the
    pub fn transfer_with_fee(ctx: Context<TransferWithFee>, amount: u64, decimals: u8) -> Result<()> {
        instructions::transfer::transfer_with_fee(ctx, amount, decimals)
    }

    /// Task 3 — read exclusively via StateWithExtensions; also the mint
    pub fn assert_supported_mint(ctx: Context<AssertSupportedMint>) -> Result<()> {
        instructions::validate::assert_supported_mint(ctx)
    }

    /// Task 4 — freeze authority thaws one account after KYC. See
    pub fn thaw_after_kyc(ctx: Context<ThawAfterKyc>) -> Result<()> {
        instructions::kyc::thaw_after_kyc(ctx)
    }

    /// Task 5 — re-issued mint carrying the base extensions forward, plus
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
        instructions::reissue_mint::create_remittance_mint_v2(
            ctx,
            decimals,
            basis_points,
            maximum_fee,
            withdraw_withheld_authority_elgamal_pubkey,
            name,
            symbol,
            uri,
        )
    }

    /// Seizes from the PUBLIC balance only. See `instructions::seize`.
    pub fn permanent_delegate_seize(
        ctx: Context<PermanentDelegateSeize>,
        amount: u64,
        decimals: u8,
    ) -> Result<()> {
        instructions::seize::permanent_delegate_seize(ctx, amount, decimals)
    }

    /// Task 6 — ConfigureAccount (owner-only). See
    pub fn configure_confidential_account(
        ctx: Context<ConfigureConfidentialAccount>,
        decryptable_zero_balance: [u8; constants::AE_CIPHERTEXT_LEN],
        maximum_pending_balance_credit_counter: u64,
    ) -> Result<()> {
        instructions::confidential::configure_confidential_account(
            ctx,
            decryptable_zero_balance,
            maximum_pending_balance_credit_counter,
        )
    }

    /// Task 6 — DepositConfidentialTokens.
    pub fn deposit_confidential(ctx: Context<DepositConfidential>, amount: u64, decimals: u8) -> Result<()> {
        instructions::confidential::deposit_confidential(ctx, amount, decimals)
    }

    /// Task 6 — ApplyPendingBalance.
    pub fn apply_pending_balance(
        ctx: Context<ApplyPendingBalance>,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; constants::AE_CIPHERTEXT_LEN],
    ) -> Result<()> {
        instructions::confidential::apply_pending_balance(
            ctx,
            expected_pending_balance_credit_counter,
            new_decryptable_available_balance,
        )
    }

    /// Task 6 — confidential Transfer.
    pub fn confidential_transfer(
        ctx: Context<ConfidentialTransfer>,
        new_source_decryptable_available_balance: [u8; constants::AE_CIPHERTEXT_LEN],
        ciphertext_lo: [u8; constants::EL_GAMAL_CIPHERTEXT_LEN],
        ciphertext_hi: [u8; constants::EL_GAMAL_CIPHERTEXT_LEN],
    ) -> Result<()> {
        instructions::confidential::confidential_transfer(
            ctx,
            new_source_decryptable_available_balance,
            ciphertext_lo,
            ciphertext_hi,
        )
    }

    /// Task 6 — WithdrawConfidentialTokens. Enforces on-chain that
    /// ApplyPendingBalance was called first.
    pub fn withdraw_confidential(
        ctx: Context<WithdrawConfidential>,
        amount: u64,
        decimals: u8,
        new_decryptable_available_balance: [u8; constants::AE_CIPHERTEXT_LEN],
    ) -> Result<()> {
        instructions::confidential::withdraw_confidential(
            ctx,
            amount,
            decimals,
            new_decryptable_available_balance,
        )
    }
}
