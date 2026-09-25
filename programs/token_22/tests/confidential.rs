use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use proofgen::{transfer::transfer_split_proof_data, withdraw::withdraw_proof_data};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use token_22::{accounts, instruction, ID};
use token_22new::{
    extension::{
        confidential_transfer::ConfidentialTransferAccount, BaseStateWithExtensions,
        ExtensionType, StateWithExtensions,
    },
    instruction::{initialize_account3, mint_to},
    state::Account as TokenAccountState,
};
use zk::{
    encryption::{
        auth_encryption::{AeCiphertext, AeKey},
        elgamal::{ElGamalCiphertext, ElGamalKeypair, ElGamalPubkey},
    },
    zk_elgamal_proof_program::{
        instruction::{close_context_state, ContextStateInfo, ProofInstruction},
        proof_data::{PubkeyValidityProofData, ZkProofData},
        state::ProofContextState,
        ID as ZK_PROGRAM_ID,
    },
};

const TOKEN_2022_PROGRAM_ID: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;
const DECIMALS: u8 = 2;
const BASIS_POINTS: u16 = 250;
const MAXIMUM_FEE: u64 = 5_000;

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

fn stage_proof<T, U>(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction_kind: ProofInstruction,
    proof: &T,
) -> Pubkey
where
    T: bytemuck::Pod + ZkProofData<U>,
    U: bytemuck::Pod,
{
    let context_len = std::mem::size_of::<ProofContextState<U>>();
    let context = Keypair::new();
    let lamports = svm.minimum_balance_for_rent_exemption(context_len);
    send(
        svm,
        payer,
        &[solana_system_interface::instruction::create_account(
            &payer.pubkey(),
            &context.pubkey(),
            lamports,
            context_len as u64,
            &ZK_PROGRAM_ID,
        )],
        &[&context],
    );
    let ix = instruction_kind.encode_verify_proof(
        Some(ContextStateInfo {
            context_state_account: &context.pubkey(),
            context_state_authority: &payer.pubkey(),
        }),
        proof,
    );
    send(
        svm,
        payer,
        &[ComputeBudgetInstruction::set_compute_unit_limit(400_000), ix],
        &[],
    );
    context.pubkey()
}

fn close_contexts(svm: &mut LiteSVM, payer: &Keypair, contexts: &[Pubkey]) {
    let ixs: Vec<Instruction> = contexts
        .iter()
        .map(|c| {
            close_context_state(
                ContextStateInfo {
                    context_state_account: c,
                    context_state_authority: &payer.pubkey(),
                },
                &payer.pubkey(),
            )
        })
        .collect();
    send(svm, payer, &ixs, &[]);
}

fn create_v2_mint(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let mint = Keypair::new();
    let fee_authority_elgamal = ElGamalKeypair::new_from_signer(payer, b"").unwrap();
    let withdraw_withheld_authority_elgamal_pubkey: [u8; 32] = fee_authority_elgamal.pubkey().into();

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

fn thaw(svm: &mut LiteSVM, payer: &Keypair, account: Pubkey, mint: Pubkey) {
    send(
        svm,
        payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::ThawAfterKyc {
                token_account: account,
                mint,
                freeze_authority: payer.pubkey(),
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::ThawAfterKyc {}.data(),
        }],
        &[],
    );
}

struct Holder {
    account: Pubkey,
    elgamal: ElGamalKeypair,
    aes: AeKey,
}

/// Opens a token account, thaws it (the v2 mint defaults new accounts to
/// frozen), then runs ConfigureAccount through the PROGRAM's own
/// instruction — not `ct_ix::configure_account` directly — staging the
/// pubkey-validity proof into a context account first, since
/// `configure_confidential_account` expects `ProofLocation::
/// ContextStateAccount` rather than `InstructionOffset`.
fn create_and_configure(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Keypair,
) -> Holder {
    let ta = Keypair::new();
    let space = ExtensionType::try_calculate_account_len::<TokenAccountState>(&[
        ExtensionType::TransferFeeAmount,
        ExtensionType::ConfidentialTransferAccount,
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
            initialize_account3(&TOKEN_2022_PROGRAM_ID, &ta.pubkey(), mint, &owner.pubkey())
                .unwrap(),
        ],
        &[&ta],
    );
    thaw(svm, payer, ta.pubkey(), *mint);

    let elgamal = ElGamalKeypair::new_from_signer(owner, b"").unwrap();
    let aes = AeKey::new_from_signer(owner, b"").unwrap();
    let proof = PubkeyValidityProofData::new(&elgamal).unwrap();
    let proof_ctx = stage_proof(svm, payer, ProofInstruction::VerifyPubkeyValidity, &proof);

    send(
        svm,
        payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::ConfigureConfidentialAccount {
                token_account: ta.pubkey(),
                mint: *mint,
                authority: owner.pubkey(),
                pubkey_validity_proof_context: proof_ctx,
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::ConfigureConfidentialAccount {
                decryptable_zero_balance: aes.encrypt(0).to_bytes(),
                maximum_pending_balance_credit_counter: 65536,
            }
            .data(),
        }],
        &[owner],
    );
    close_contexts(svm, payer, &[proof_ctx]);

    Holder {
        account: ta.pubkey(),
        elgamal,
        aes,
    }
}

fn read_ct(svm: &LiteSVM, account: &Pubkey) -> ConfidentialTransferAccount {
    let acct = svm.get_account(account).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&acct.data).unwrap();
    *state.get_extension::<ConfidentialTransferAccount>().unwrap()
}

fn available_balance(ct: &ConfidentialTransferAccount, elgamal: &ElGamalKeypair) -> u64 {
    let ciphertext: ElGamalCiphertext = ct.available_balance.try_into().unwrap();
    elgamal.secret().decrypt_u32(&ciphertext).unwrap()
}

fn pending_balance(ct: &ConfidentialTransferAccount, elgamal: &ElGamalKeypair) -> u64 {
    let lo: ElGamalCiphertext = ct.pending_balance_lo.try_into().unwrap();
    let hi: ElGamalCiphertext = ct.pending_balance_hi.try_into().unwrap();
    let lo = elgamal.secret().decrypt_u32(&lo).unwrap();
    let hi = elgamal.secret().decrypt_u32(&hi).unwrap();
    lo + (hi << 16)
}

/// Unchanged from your original test: DepositConfidentialTokens and
/// ApplyPendingBalance already existed as program instructions before this
/// scaffold, with exactly this account/argument shape.
fn deposit(svm: &mut LiteSVM, payer: &Keypair, mint: Pubkey, holder: &Holder, amount: u64) {
    send(
        svm,
        payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::DepositConfidential {
                token_account: holder.account,
                mint,
                authority: payer.pubkey(),
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::DepositConfidential { amount, decimals: DECIMALS }.data(),
        }],
        &[],
    );
}

fn apply_pending(svm: &mut LiteSVM, payer: &Keypair, holder: &Holder, owner: &Keypair) {
    let ct = read_ct(svm, &holder.account);
    let counter: u64 = ct.pending_balance_credit_counter.into();
    let new_available =
        available_balance(&ct, &holder.elgamal) + pending_balance(&ct, &holder.elgamal);
    let ciphertext: [u8; 36] = holder.aes.encrypt(new_available).to_bytes();

    send(
        svm,
        payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::ApplyPendingBalance {
                token_account: holder.account,
                authority: owner.pubkey(),
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::ApplyPendingBalance {
                expected_pending_balance_credit_counter: counter,
                new_decryptable_available_balance: ciphertext,
            }
            .data(),
        }],
        &[owner],
    );
}

#[test]
fn full_confidential_lifecycle_through_the_program() {
    let (mut svm, payer) = setup();
    let mint = create_v2_mint(&mut svm, &payer);

    let alice_owner = payer.insecure_clone();
    let bob_owner = Keypair::new();
    svm.airdrop(&bob_owner.pubkey(), 10_000_000_000).unwrap();

    let alice = create_and_configure(&mut svm, &payer, &mint.pubkey(), &alice_owner);
    let bob = create_and_configure(&mut svm, &payer, &mint.pubkey(), &bob_owner);

    send(
        &mut svm,
        &payer,
        &[mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &alice.account,
            &payer.pubkey(),
            &[],
            10_000,
        )
        .unwrap()],
        &[],
    );

    deposit(&mut svm, &payer, mint.pubkey(), &alice, 10_000);
    apply_pending(&mut svm, &payer, &alice, &alice_owner);
    let alice_available = available_balance(&read_ct(&svm, &alice.account), &alice.elgamal);
    assert_eq!(alice_available, 10_000);

    // ---- confidential Transfer, through our own program instruction ----
    let transfer_amount = 2_500u64;
    let ct = read_ct(&svm, &alice.account);
    let current_available: ElGamalCiphertext = ct.available_balance.try_into().unwrap();
    let current_decryptable: AeCiphertext = ct.decryptable_available_balance.try_into().unwrap();
    let bob_pubkey: ElGamalPubkey = read_ct(&svm, &bob.account).elgamal_pubkey.try_into().unwrap();

    let proofs = transfer_split_proof_data(
        &current_available,
        &current_decryptable,
        transfer_amount,
        &alice.elgamal,
        &alice.aes,
        &bob_pubkey,
        None,
    )
    .unwrap();

    let eq_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &proofs.equality_proof_data,
    );
    let val_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyBatchedGroupedCiphertext3HandlesValidity,
        &proofs.ciphertext_validity_proof_data_with_ciphertext.proof_data,
    );
    let range_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyBatchedRangeProofU128,
        &proofs.range_proof_data,
    );

    let new_alice_decryptable: [u8; 36] =
        alice.aes.encrypt(alice_available - transfer_amount).to_bytes();
    let ciphertext_lo: [u8; 64] = bytemuck::bytes_of(
        &proofs.ciphertext_validity_proof_data_with_ciphertext.ciphertext_lo,
    )
    .try_into()
    .expect("ElGamal ciphertext is 64 bytes — adjust EL_GAMAL_CIPHERTEXT_LEN if this panics");
    let ciphertext_hi: [u8; 64] = bytemuck::bytes_of(
        &proofs.ciphertext_validity_proof_data_with_ciphertext.ciphertext_hi,
    )
    .try_into()
    .unwrap();

    send(
        &mut svm,
        &payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::ConfidentialTransfer {
                source: alice.account,
                mint: mint.pubkey(),
                destination: bob.account,
                authority: alice_owner.pubkey(),
                equality_proof_context: eq_ctx,
                ciphertext_validity_proof_context: val_ctx,
                range_proof_context: range_ctx,
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::ConfidentialTransfer {
                new_source_decryptable_available_balance: new_alice_decryptable,
                ciphertext_lo,
                ciphertext_hi,
            }
            .data(),
        }],
        &[&alice_owner],
    );
    close_contexts(&mut svm, &payer, &[eq_ctx, val_ctx, range_ctx]);

    let bob_ct = read_ct(&svm, &bob.account);
    assert_eq!(pending_balance(&bob_ct, &bob.elgamal), transfer_amount);
    assert_eq!(available_balance(&bob_ct, &bob.elgamal), 0);

    apply_pending(&mut svm, &payer, &bob, &bob_owner);
    let bob_ct = read_ct(&svm, &bob.account);
    assert_eq!(available_balance(&bob_ct, &bob.elgamal), transfer_amount);

    // ---- WithdrawConfidentialTokens, through our own program instruction,
    //      after ApplyPendingBalance (enforced on-chain — see task 3/6 in
    //      confidential.rs) ----
    let withdraw_amount = 1_000u64;
    let bob_ct = read_ct(&svm, &bob.account);
    let bob_available = available_balance(&bob_ct, &bob.elgamal);
    let bob_current: ElGamalCiphertext = bob_ct.available_balance.try_into().unwrap();

    let wproofs =
        withdraw_proof_data(&bob_current, bob_available, withdraw_amount, &bob.elgamal).unwrap();

    let weq_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &wproofs.equality_proof_data,
    );
    let wrange_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyBatchedRangeProofU64,
        &wproofs.range_proof_data,
    );

    let new_bob_decryptable: [u8; 36] =
        bob.aes.encrypt(bob_available - withdraw_amount).to_bytes();

    send(
        &mut svm,
        &payer,
        &[Instruction {
            program_id: ID,
            accounts: accounts::WithdrawConfidential {
                token_account: bob.account,
                mint: mint.pubkey(),
                authority: bob_owner.pubkey(),
                equality_proof_context: weq_ctx,
                range_proof_context: wrange_ctx,
                token_program: TOKEN_2022_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: instruction::WithdrawConfidential {
                amount: withdraw_amount,
                decimals: DECIMALS,
                new_decryptable_available_balance: new_bob_decryptable,
            }
            .data(),
        }],
        &[&bob_owner],
    );
    close_contexts(&mut svm, &payer, &[weq_ctx, wrange_ctx]);

    let acct = svm.get_account(&bob.account).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&acct.data).unwrap();
    assert_eq!(state.base.amount, withdraw_amount);

    let bob_ct = read_ct(&svm, &bob.account);
    assert_eq!(
        available_balance(&bob_ct, &bob.elgamal),
        transfer_amount - withdraw_amount
    );
    println!(
        "lifecycle complete: bob withdrew {withdraw_amount}, confidential balance left {}",
        transfer_amount - withdraw_amount
    );
}

#[test]
fn withdraw_is_refused_if_pending_balance_was_never_applied() {
    let (mut svm, payer) = setup();
    let mint = create_v2_mint(&mut svm, &payer);
    let alice_owner = payer.insecure_clone();

    let alice = create_and_configure(&mut svm, &payer, &mint.pubkey(), &alice_owner);

    send(
        &mut svm,
        &payer,
        &[mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint.pubkey(),
            &alice.account,
            &payer.pubkey(),
            &[],
            5_000,
        )
        .unwrap()],
        &[],
    );
    // Deposit lands in PENDING balance and is never applied — the
    // program's on-chain check in `withdraw_confidential` must refuse a
    // withdrawal here rather than silently ignore the pending credit.
    deposit(&mut svm, &payer, mint.pubkey(), &alice, 5_000);

    let ct = read_ct(&svm, &alice.account);
    let available = available_balance(&ct, &alice.elgamal); // still 0
    let current: ElGamalCiphertext = ct.available_balance.try_into().unwrap();
    let wproofs = withdraw_proof_data(&current, available, 0, &alice.elgamal).unwrap();

    let weq_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &wproofs.equality_proof_data,
    );
    let wrange_ctx = stage_proof(
        &mut svm,
        &payer,
        ProofInstruction::VerifyBatchedRangeProofU64,
        &wproofs.range_proof_data,
    );

    let bh = svm.latest_blockhash();
    let ix = Instruction {
        program_id: ID,
        accounts: accounts::WithdrawConfidential {
            token_account: alice.account,
            mint: mint.pubkey(),
            authority: alice_owner.pubkey(),
            equality_proof_context: weq_ctx,
            range_proof_context: wrange_ctx,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::WithdrawConfidential {
            amount: 0,
            decimals: DECIMALS,
            new_decryptable_available_balance: alice.aes.encrypt(0u64).to_bytes(),
        }
        .data(),
    };
    let mut tx = Transaction::new_unsigned(Message::new(&[ix], Some(&payer.pubkey())));
    tx.try_sign(&[&payer, &alice_owner], bh).unwrap();
    match svm.send_transaction(tx) {
        Ok(_) => panic!("withdraw succeeded despite an un-applied pending balance"),
        Err(e) => {
            let logs = e.meta.logs.join("\n");
            println!("correctly refused:\n{logs}");
            assert!(logs.contains("PendingBalanceNotApplied"));
        }
    }
}