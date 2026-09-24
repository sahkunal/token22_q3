use anchor_lang::prelude::*;

#[error_code]
pub enum RemittanceError {
    #[msg("mint carries an extension this program has not been written to handle")]
    UnsupportedExtension,
    #[msg("mint does not carry a TransferFeeConfig extension")]
    MissingTransferFeeConfig,
    #[msg("could not compute the transfer fee for this epoch/amount")]
    FeeCalculationFailed,
    #[msg("token account does not carry a ConfidentialTransferAccount extension")]
    MissingConfidentialExtension,
    #[msg("apply_pending_balance must be called before a confidential withdrawal")]
    PendingBalanceNotApplied,
}
