use anchor_spl::token_interface::spl_token_2022::extension::ExtensionType;
pub const BASE_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::MintCloseAuthority,
    ExtensionType::MetadataPointer,
    ExtensionType::TransferFeeConfig,
    ExtensionType::DefaultAccountState,
];

pub const CONFIDENTIAL_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::MintCloseAuthority,
    ExtensionType::MetadataPointer,
    ExtensionType::TransferFeeConfig,
    ExtensionType::DefaultAccountState,
    ExtensionType::PermanentDelegate,
    ExtensionType::ConfidentialTransferMint,
    ExtensionType::ConfidentialTransferFeeConfig,
];

pub const AE_CIPHERTEXT_LEN: usize = 36;

pub const EL_GAMAL_CIPHERTEXT_LEN: usize = 64;
