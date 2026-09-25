//! Task 5: the re-issued mint (base extensions + PermanentDelegate +
//! confidential transfers, manual approval) and the seizure path against
//! the public balance.

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{
        confidential_transfer::ConfidentialTransferMint, permanent_delegate::PermanentDelegate,
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    state::{Account as TokenAccountState, Mint as MintState},
};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use token_22::{accounts, instruction, ID};
use token_22new::instruction::{initialize_account3, mint_to};

const TOKEN_2022_PROGRAM_ID: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;
const DECIMALS: u8 = 6;
const BASIS_POINTS: u16 = 250;
const MAXIMUM_FEE: u64 = 1_000;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/token_22.so");
    svm.add_program_from_file(ID, path).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction], extra: &[&Keypair]) {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra);
    let bh = svm.latest_blockhash();
    let mut tx = Transaction::new_unsigned(Message::new(ixs, Some(&payer.pubkey())));
    tx.try_sign(&signers, bh).unwrap();
    if let Err(e) = svm.send_transaction(tx) {
        panic!("tx failed: {:?}\nlogs: {:#?}", e.err, e.meta.logs);
    }
}

fn plain_account(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
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

fn read_balance(svm: &LiteSVM, addr: &Pubkey) -> u64 {
    let acct = svm.get_account(addr).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&acct.data).unwrap();
    state.base.amount
}

fn create_v2_mint(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let mint = Keypair::new();
    // A throwaway 32-byte value stands in for a real ElGamal pubkey here —
    // fine for exercising the seize path, which never touches confidential
    // state at all (see the gap analysis in reissue_mint.rs).
    let withdraw_withheld_authority_elgamal_pubkey = [7u8; 32];

    send(
        svm,
        payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::CreateRemittanceMintV2 {
                payer: payer.pubkey(),
                mint: mint.pubkey(),
                token_program: TOKEN_2022_PROGRAM_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: instruction::CreateRemittanceMintV2 {
                decimals: DECIMALS,
                basis_points: BASIS_POINTS,
                maximum_fee: MAXIMUM_FEE,
                withdraw_withheld_authority_elgamal_pubkey,
                name: "Remittance USD".to_string(),
                symbol: "rUSD".to_string(),
                uri: "https://example.com/rusd.json".to_string(),
            }
            .data(),
        }],
        &[&mint],
    );
    mint
}

#[test]
fn v2_mint_carries_the_full_seven_extension_stack() {
    let (mut svm, payer) = setup();
    let mint = create_v2_mint(&mut svm, &payer);

    let account = svm.get_account(&mint.pubkey()).unwrap();
    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
    let extensions = state.get_extension_types().unwrap();
    println!("v2 mint extensions = {extensions:?}");

    for required in [
        ExtensionType::MintCloseAuthority,
        ExtensionType::MetadataPointer,
        ExtensionType::TransferFeeConfig,
        ExtensionType::DefaultAccountState,
        ExtensionType::PermanentDelegate,
        ExtensionType::ConfidentialTransferMint,
        ExtensionType::ConfidentialTransferFeeConfig,
    ] {
        assert!(extensions.contains(&required), "missing {required:?}");
    }

    let delegate = state.get_extension::<PermanentDelegate>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(delegate.delegate),
        Some(payer.pubkey())
    );

    // approve_policy = manual: auto_approve_new_accounts must be false.
    let ct_mint = state.get_extension::<ConfidentialTransferMint>().unwrap();
    assert!(!bool::from(ct_mint.auto_approve_new_accounts));
}

#[test]
fn a_permanent_delegate_moves_public_balance_without_consent() {
    let (mut svm, payer) = setup();
    let mint = create_v2_mint(&mut svm, &payer);

    // A holder who has nothing to do with the mint authority.
    let victim = Keypair::new();
    svm.airdrop(&victim.pubkey(), 1_000_000_000).unwrap();
    let victim_account = plain_account(&mut svm, &payer, &mint.pubkey(), &victim.pubkey());
    let attacker_account = plain_account(&mut svm, &payer, &mint.pubkey(), &payer.pubkey());

    // The v2 mint carries DefaultAccountState(Frozen) forward from the base
    // extension set, so both fresh accounts start frozen — thaw them (task
    // 4's path) before minting/transferring, exactly as a real KYC flow
    // would after clearing each holder.
    let thaw = |account: Pubkey| Instruction {
        program_id: ID,
        accounts: accounts::ThawAfterKyc {
            token_account: account,
            mint: mint.pubkey(),
            freeze_authority: payer.pubkey(),
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::ThawAfterKyc {}.data(),
    };
    send(&mut svm, &payer, &[thaw(victim_account)], &[]);
    send(&mut svm, &payer, &[thaw(attacker_account)], &[]);

    send(
        &mut svm,
        &payer,
        &[mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &victim_account,
            &payer.pubkey(),
            &[],
            1_000,
        )
        .unwrap()],
        &[],
    );

    assert_eq!(read_balance(&svm, &victim_account), 1_000);

    // The victim never signs this transaction — that's the entire point of
    // a permanent delegate.
    send(
        &mut svm,
        &payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::PermanentDelegateSeize {
                source: victim_account,
                mint: mint.pubkey(),
                destination: attacker_account,
                permanent_delegate: payer.pubkey(),
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::PermanentDelegateSeize {
                amount: 1_000,
                decimals: DECIMALS,
            }
            .data(),
        }],
        &[],
    );

    assert_eq!(read_balance(&svm, &victim_account), 0);
    assert_eq!(read_balance(&svm, &attacker_account), 1_000);
    println!("permanent delegate moved 1000 with no approval and no holder signature");
}
