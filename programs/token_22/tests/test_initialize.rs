
use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{
        default_account_state::instruction::initialize_default_account_state,
        mint_close_authority::MintCloseAuthority, transfer_fee::TransferFeeConfig,
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    instruction::initialize_mint2,
    state::{AccountState, Mint as MintState},
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use token_22::{accounts, instruction, ID};
 
const TOKEN_2022_PROGRAM_ID: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;
const DECIMALS: u8 = 6;
const BASIS_POINTS: u16 = 250;
const MAXIMUM_FEE: u64 = 1_000;
 
const BASE_MINT_LEN: usize = 82;
 const BASE_ACCOUNT_LEN: usize = 165;
 
fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
 
    let program_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/token_22.so");
    assert!(
        program_path.exists(),
        "program binary not found at {}. Run cargo-build-sbf first.",
        program_path.display()
    );
    svm.add_program_from_file(ID, program_path).unwrap();
 
    (svm, payer)
}
fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction, extra_signers: &[&Keypair]) {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);
 
    let blockhash = svm.latest_blockhash();
    let mut transaction = Transaction::new_unsigned(Message::new(&[ix], Some(&payer.pubkey())));
    transaction.try_sign(&signers, blockhash).unwrap();
 
    if let Err(err) = svm.send_transaction(transaction) {
        panic!("transaction failed:\n{err:#?}");
    }
}
 
fn send_expecting_failure(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> String {
    let blockhash = svm.latest_blockhash();
    let mut transaction = Transaction::new_unsigned(Message::new(&[ix], Some(&payer.pubkey())));
    transaction.try_sign(&[payer], blockhash).unwrap();
 
    match svm.send_transaction(transaction) {
        Ok(_) => panic!("expected the transaction to fail, but it succeeded"),
        Err(err) => format!("{err:#?}"),
    }
}
 
fn create_mint_declarative_ix(payer: &Pubkey, mint: &Pubkey, decimals: u8) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::CreateMintDeclarative {
            payer: *payer,
            mint: *mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateMintDeclarative { decimals }.data(),
    }
}
 
fn assert_supported_mint_ix(mint: &Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::AssertSupportedMint {
            mint: *mint,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::AssertSupportedMint {}.data(),
    }
}
 
#[test]
fn declarative_mint_has_the_size_the_extension_set_requires() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
 
    send(
        &mut svm,
        &payer,
        create_mint_declarative_ix(&payer.pubkey(), &mint.pubkey(), DECIMALS),
        &[&mint],
    );
 
    let account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(account.owner, TOKEN_2022_PROGRAM_ID);
    let expected = ExtensionType::try_calculate_account_len::<MintState>(&[
        ExtensionType::MintCloseAuthority,
        ExtensionType::MetadataPointer,
    ])
    .unwrap();
 
    assert_eq!(account.data.len(), expected);
    assert_eq!(account.data.len(), 270);
}
 
#[test]
fn the_first_extension_costs_far_more_than_the_second() {
    let close_only =
        ExtensionType::try_calculate_account_len::<MintState>(&[ExtensionType::MintCloseAuthority])
            .unwrap();
    let close_and_pointer = ExtensionType::try_calculate_account_len::<MintState>(&[
        ExtensionType::MintCloseAuthority,
        ExtensionType::MetadataPointer,
    ])
    .unwrap();
 
    assert_eq!(close_only - BASE_MINT_LEN, 120);
    assert_eq!(close_and_pointer - close_only, 68);
    assert!(close_only > BASE_ACCOUNT_LEN);
}
 
#[test]
fn declarative_mint_initializes_exactly_the_declared_extensions() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
 
    send(
        &mut svm,
        &payer,
        create_mint_declarative_ix(&payer.pubkey(), &mint.pubkey(), DECIMALS),
        &[&mint],
    );
 
    let account = svm.get_account(&mint.pubkey()).unwrap();
    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
 
    assert_eq!(state.base.decimals, DECIMALS);
    assert_eq!(
        state.get_extension_types().unwrap(),
        vec![
            ExtensionType::MetadataPointer,
            ExtensionType::MintCloseAuthority,
        ]
    );
 
    let close_authority = state.get_extension::<MintCloseAuthority>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(close_authority.close_authority),
        Some(payer.pubkey())
    );
}
 
#[test]
fn manual_path_produces_a_working_transfer_fee_mint() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
 
    let ix = Instruction {
        program_id: ID,
        accounts: accounts::CreateMintWithFee {
            payer: payer.pubkey(),
            mint: mint.pubkey(),
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateMintWithFee {
            decimals: DECIMALS,
            basis_points: BASIS_POINTS,
            maximum_fee: MAXIMUM_FEE,
        }
        .data(),
    };
    send(&mut svm, &payer, ix, &[&mint]);
 
    let account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(account.data.len(), 314);
 
    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
    assert_eq!(
        state.get_extension_types().unwrap(),
        vec![
            ExtensionType::MintCloseAuthority,
            ExtensionType::TransferFeeConfig,
        ]
    );
 
    let config = state.get_extension::<TransferFeeConfig>().unwrap();
    assert_eq!(
        u16::from(config.newer_transfer_fee.transfer_fee_basis_points),
        BASIS_POINTS
    );
    assert_eq!(
        u64::from(config.newer_transfer_fee.maximum_fee),
        MAXIMUM_FEE
    );
}
 
#[test]
fn transfer_fee_on_the_mint_enlarges_every_token_account() {
    let required =
        ExtensionType::get_required_init_account_extensions(&[ExtensionType::TransferFeeConfig]);
    assert_eq!(required, vec![ExtensionType::TransferFeeAmount]);
 
    let holder_len = ExtensionType::try_calculate_account_len::<
        anchor_spl::token_interface::spl_token_2022::state::Account,
    >(&required)
    .unwrap();
    assert!(holder_len > BASE_ACCOUNT_LEN);
}
 
#[test]
fn validation_accepts_a_mint_whose_extensions_are_allowlisted() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
 
    send(
        &mut svm,
        &payer,
        create_mint_declarative_ix(&payer.pubkey(), &mint.pubkey(), 9),
        &[&mint],
    );
    send(
        &mut svm,
        &payer,
        assert_supported_mint_ix(&mint.pubkey()),
        &[],
    );
}
 
#[test]
fn validation_rejects_a_mint_carrying_an_unlisted_extension() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
    let space = ExtensionType::try_calculate_account_len::<MintState>(&[
        ExtensionType::DefaultAccountState,
    ])
    .unwrap();
    let lamports = svm.minimum_balance_for_rent_exemption(space);
 
    let instructions = [
        solana_system_interface::instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            lamports,
            space as u64,
            &TOKEN_2022_PROGRAM_ID,
        ),
    
        initialize_default_account_state(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &AccountState::Frozen,
        )
        .unwrap(),
        initialize_mint2(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &payer.pubkey(),
            Some(&payer.pubkey()),
            DECIMALS,
        )
        .unwrap(),
    ];
 
    let blockhash = svm.latest_blockhash();
    let mut transaction =
        Transaction::new_unsigned(Message::new(&instructions, Some(&payer.pubkey())));
    transaction.try_sign(&[&payer, &mint], blockhash).unwrap();
    svm.send_transaction(transaction).unwrap();
 
    let logs = send_expecting_failure(&mut svm, &payer, assert_supported_mint_ix(&mint.pubkey()));
    assert!(
        logs.contains("UnsupportedExtension"),
        "rejected for the wrong reason:\n{logs}"
    );
}
 
#[test]
fn validation_rejects_a_forged_mint_owned_by_another_program() {
    let (mut svm, payer) = setup();
 
    // First make a genuine mint so there are valid bytes to copy.
    let real_mint = Keypair::new();
    send(
        &mut svm,
        &payer,
        create_mint_declarative_ix(&payer.pubkey(), &real_mint.pubkey(), DECIMALS),
        &[&real_mint],
    );
    let real = svm.get_account(&real_mint.pubkey()).unwrap();
 
    // Plant those exact bytes in an account owned by an unrelated program.
    let attacker_program = Pubkey::new_unique();
    let forged = Pubkey::new_unique();
    svm.set_account(
        forged,
        Account {
            lamports: real.lamports,
            data: real.data.clone(),
            owner: attacker_program,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
 
    assert!(StateWithExtensions::<MintState>::unpack(&real.data).is_ok());
 
    let logs = send_expecting_failure(&mut svm, &payer, assert_supported_mint_ix(&forged));
    assert!(
        logs.contains("ConstraintOwner") || logs.contains("2004"),
        "forged mint was accepted, so the owner check is missing:\n{logs}"
    );
}
 