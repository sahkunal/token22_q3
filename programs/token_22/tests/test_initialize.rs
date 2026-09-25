use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{
        default_account_state::instruction::initialize_default_account_state,
        metadata_pointer::MetadataPointer,
        mint_close_authority::MintCloseAuthority,
        transfer_fee::TransferFeeConfig,
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    instruction::initialize_mint2,
    state::{Account as TokenAccountState, AccountState, Mint as MintState},
};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use token_22::{accounts, instruction, ID};
use token_22new::instruction::initialize_account3;

const TOKEN_2022_PROGRAM_ID: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;
const DECIMALS: u8 = 6;
const BASIS_POINTS: u16 = 250;
const MAXIMUM_FEE: u64 = 1_000;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let program_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/token_22.so");
    assert!(
        program_path.exists(),
        "program binary not found at {}. Run cargo-build-sbf / anchor build first.",
        program_path.display()
    );
    svm.add_program_from_file(ID, program_path).unwrap();

    (svm, payer)
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction], extra_signers: &[&Keypair]) {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);

    let blockhash = svm.latest_blockhash();
    let mut transaction = Transaction::new_unsigned(Message::new(ixs, Some(&payer.pubkey())));
    transaction.try_sign(&signers, blockhash).unwrap();

    if let Err(err) = svm.send_transaction(transaction) {
        panic!("transaction failed:\n{err:#?}");
    }
}

fn send_expecting_failure(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ixs: &[Instruction],
    extra_signers: &[&Keypair],
) -> String {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);

    let blockhash = svm.latest_blockhash();
    let mut transaction = Transaction::new_unsigned(Message::new(ixs, Some(&payer.pubkey())));
    transaction.try_sign(&signers, blockhash).unwrap();

    match svm.send_transaction(transaction) {
        Ok(_) => panic!("expected the transaction to fail, but it succeeded"),
        Err(err) => err.meta.logs.join("\n"),
    }
}

fn create_remittance_mint_ix(
    payer: &Pubkey,
    mint: &Pubkey,
    decimals: u8,
    basis_points: u16,
    maximum_fee: u64,
    name: &str,
    symbol: &str,
    uri: &str,
) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::CreateRemittanceMint {
            payer: *payer,
            mint: *mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateRemittanceMint {
            decimals,
            basis_points,
            maximum_fee,
            name: name.to_string(),
            symbol: symbol.to_string(),
            uri: uri.to_string(),
        }
        .data(),
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

fn create_remittance_mint(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let mint = Keypair::new();
    send(
        svm,
        payer,
        &[create_remittance_mint_ix(
            &payer.pubkey(),
            &mint.pubkey(),
            DECIMALS,
            BASIS_POINTS,
            MAXIMUM_FEE,
            "Remittance USD",
            "rUSD",
            "https://example.com/rusd.json",
        )],
        &[&mint],
    );
    mint
}

fn open_account(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    let ta = Keypair::new();
    let space = ExtensionType::try_calculate_account_len::<TokenAccountState>(&[
        ExtensionType::TransferFeeAmount,
    ])
    .unwrap();
    let lamports = svm.minimum_balance_for_rent_exemption(space);
    send(
        svm,
        payer,
        &[
            solana_system_interface::instruction::create_account(
                &payer.pubkey(),
                &ta.pubkey(),
                lamports,
                space as u64,
                &TOKEN_2022_PROGRAM_ID,
            ),
            initialize_account3(&TOKEN_2022_PROGRAM_ID, &ta.pubkey(), mint, owner).unwrap(),
        ],
        &[&ta],
    );
    ta.pubkey()
}

fn thaw_ix(token_account: &Pubkey, mint: &Pubkey, freeze_authority: &Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::ThawAfterKyc {
            token_account: *token_account,
            mint: *mint,
            freeze_authority: *freeze_authority,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::ThawAfterKyc {}.data(),
    }
}

fn transfer_with_fee_ix(
    source: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Pubkey,
    amount: u64,
    decimals: u8,
) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::TransferWithFee {
            source: *source,
            mint: *mint,
            destination: *destination,
            authority: *authority,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::TransferWithFee { amount, decimals }.data(),
    }
}

fn read_account_state(svm: &LiteSVM, addr: &Pubkey) -> (u64, AccountState) {
    let acct = svm.get_account(addr).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&acct.data).unwrap();
    (state.base.amount, state.base.state)
}

// ---------------------------------------------------------------------
// Task 1 — mint creation
// ---------------------------------------------------------------------

#[test]
fn remittance_mint_carries_all_four_extensions() {
    let (mut svm, payer) = setup();
    let mint = create_remittance_mint(&mut svm, &payer);

    let account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(account.owner, TOKEN_2022_PROGRAM_ID);

    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
    assert_eq!(state.base.decimals, DECIMALS);

    let extensions = state.get_extension_types().unwrap();
    println!("remittance mint extensions = {extensions:?}");
    for required in [
        ExtensionType::MintCloseAuthority,
        ExtensionType::MetadataPointer,
        ExtensionType::TransferFeeConfig,
        ExtensionType::DefaultAccountState,
    ] {
        assert!(extensions.contains(&required), "missing {required:?}");
    }

    let close_authority = state.get_extension::<MintCloseAuthority>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(close_authority.close_authority),
        Some(payer.pubkey())
    );

    let fee_config = state.get_extension::<TransferFeeConfig>().unwrap();
    assert_eq!(
        u16::from(fee_config.newer_transfer_fee.transfer_fee_basis_points),
        BASIS_POINTS
    );
    assert_eq!(u64::from(fee_config.newer_transfer_fee.maximum_fee), MAXIMUM_FEE);

    let metadata_pointer = state.get_extension::<MetadataPointer>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(metadata_pointer.metadata_address),
        Some(mint.pubkey())
    );
}

#[test]
fn new_accounts_are_frozen_until_thawed() {
    let (mut svm, payer) = setup();
    let mint = create_remittance_mint(&mut svm, &payer);
    let holder = open_account(&mut svm, &payer, &mint.pubkey(), &payer.pubkey());
    let other = open_account(&mut svm, &payer, &mint.pubkey(), &payer.pubkey());

    let (_, state) = read_account_state(&svm, &holder);
    assert_eq!(state, AccountState::Frozen);

    // Any transfer out of a frozen account fails, KYC or not.
    let logs = send_expecting_failure(
        &mut svm,
        &payer,
        &[transfer_with_fee_ix(
            &holder,
            &mint.pubkey(),
            &other,
            &payer.pubkey(),
            10,
            DECIMALS,
        )],
        &[],
    );
    assert!(
        logs.contains("AccountFrozen") || logs.contains("frozen"),
        "expected a frozen-account failure:\n{logs}"
    );

    send(&mut svm, &payer, &[thaw_ix(&holder, &mint.pubkey(), &payer.pubkey())], &[]);
    let (_, state) = read_account_state(&svm, &holder);
    assert_eq!(state, AccountState::Initialized);
    let (_, other_state) = read_account_state(&svm, &other);
    assert_eq!(other_state, AccountState::Frozen);
}

#[test]
fn transfer_with_fee_withholds_the_correct_amount() {
    let (mut svm, payer) = setup();
    let mint = create_remittance_mint(&mut svm, &payer);

    let sender = open_account(&mut svm, &payer, &mint.pubkey(), &payer.pubkey());
    let receiver = open_account(&mut svm, &payer, &mint.pubkey(), &payer.pubkey());
    send(&mut svm, &payer, &[thaw_ix(&sender, &mint.pubkey(), &payer.pubkey())], &[]);
    send(&mut svm, &payer, &[thaw_ix(&receiver, &mint.pubkey(), &payer.pubkey())], &[]);

    send(
        &mut svm,
        &payer,
        &[token_22new::instruction::mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &sender,
            &payer.pubkey(),
            &[],
            100_000,
        )
        .unwrap()],
        &[],
    );

    let amount = 40_000u64;
    let expected_fee = std::cmp::min(
        amount * u64::from(BASIS_POINTS) / 10_000,
        MAXIMUM_FEE,
    );

    send(
        &mut svm,
        &payer,
        &[transfer_with_fee_ix(
            &sender,
            &mint.pubkey(),
            &receiver,
            &payer.pubkey(),
            amount,
            DECIMALS,
        )],
        &[],
    );

    let (sender_balance, _) = read_account_state(&svm, &sender);
    let (receiver_balance, _) = read_account_state(&svm, &receiver);
    assert_eq!(sender_balance, 100_000 - amount);
    assert_eq!(receiver_balance, amount - expected_fee);
    println!("transferred {amount}, fee withheld {expected_fee}");
}

#[test]
fn validation_accepts_the_remittance_mint() {
    let (mut svm, payer) = setup();
    let mint = create_remittance_mint(&mut svm, &payer);
    send(&mut svm, &payer, &[assert_supported_mint_ix(&mint.pubkey())], &[]);
}

#[test]
fn validation_rejects_a_mint_carrying_an_unlisted_extension() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
    // NonTransferable is deliberately outside both BASE_EXTENSIONS and
    // CONFIDENTIAL_EXTENSIONS — a mint carrying it must be rejected.
    let space =
        ExtensionType::try_calculate_account_len::<MintState>(&[ExtensionType::NonTransferable])
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
        anchor_spl::token_interface::spl_token_2022::instruction::initialize_non_transferable_mint(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
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
    send(&mut svm, &payer, &instructions, &[&mint]);

    let logs =
        send_expecting_failure(&mut svm, &payer, &[assert_supported_mint_ix(&mint.pubkey())], &[]);
    assert!(
        logs.contains("UnsupportedExtension"),
        "rejected for the wrong reason:\n{logs}"
    );
}

#[test]
fn validation_accepts_a_mint_carrying_only_default_account_state() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
    // DefaultAccountState alone (no MintCloseAuthority/MetadataPointer/
    // TransferFeeConfig alongside it) is still a member of BASE_EXTENSIONS,
    // so this documents that the allowlist checks per-extension
    // membership, not "exactly this fixed set".
    let space =
        ExtensionType::try_calculate_account_len::<MintState>(&[ExtensionType::DefaultAccountState])
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
    send(&mut svm, &payer, &instructions, &[&mint]);

    send(&mut svm, &payer, &[assert_supported_mint_ix(&mint.pubkey())], &[]);
}
