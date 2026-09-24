# token_22 — remittance stablecoin scaffold (Token-2022)

An Anchor program scaffold for a remittance stablecoin: protocol-level
transfer fees, KYC-gated account freezing, self-describing on-chain
metadata, a mint-close authority, a seizure authority, and full
confidential transfers. Written to close the specific gaps found in a
review of an earlier draft of this program (see "What changed" below).

## Layout

```
programs/token_22/src/
  lib.rs                     thin #[program] mod — one line per instruction,
                              delegates to instructions/
  constants.rs                extension-set manifests + AE_CIPHERTEXT_LEN
  errors.rs                   RemittanceError
  instructions/
    mod.rs                    re-exports every instruction module
    create_mint.rs             task 1 — create_remittance_mint
    transfer.rs                 task 2 — transfer_with_fee
    validate.rs                  task 3 — assert_supported_mint
    kyc.rs                        task 4 — thaw_after_kyc
    reissue_mint.rs                task 5 — create_remittance_mint_v2
    seize.rs                        permanent_delegate_seize
    confidential.rs                  task 6 — full confidential lifecycle
```

Each instruction file owns both its handler function and its `Accounts`
struct, so you can read/modify/test one task without touching the others.
`lib.rs` never contains instruction logic — it's purely the
`declare_id!` + the `#[program] mod` dispatch table.

## What each task maps to

| Task | Instruction | File |
|---|---|---|
| 1. Mint stacking TransferFeeConfig + MetadataPointer(self) + DefaultAccountState(frozen) + MintCloseAuthority, sized via `try_calculate_account_len`, extensions before InitializeMint | `create_remittance_mint` | `instructions/create_mint.rs` |
| 2. Transfer via `transfer_checked_with_fee`, fee from `calculate_epoch_fee` | `transfer_with_fee` | `instructions/transfer.rs` |
| 3. Reads exclusively via `StateWithExtensions` | `assert_supported_mint` (and the read inside `withdraw_confidential`) | `instructions/validate.rs`, `instructions/confidential.rs` |
| 4. Freeze-authority thaw, separate from mint-level default state | `thaw_after_kyc` | `instructions/kyc.rs` |
| 5. Re-issued mint: PermanentDelegate + confidential transfers, manual approval | `create_remittance_mint_v2` | `instructions/reissue_mint.rs` |
| — | `permanent_delegate_seize` (public balance only) | `instructions/seize.rs` |
| 6. ConfigureAccount / Deposit / ApplyPendingBalance / Transfer / Withdraw | `configure_confidential_account`, `deposit_confidential`, `apply_pending_balance`, `confidential_transfer`, `withdraw_confidential` | `instructions/confidential.rs` |

## What changed from the earlier draft (gap fixes)

- **MetadataPointer now points at the mint itself.** It previously pointed
  at the payer's pubkey, which defeats the entire point of a
  self-describing mint.
- **On-chain metadata (name/symbol/uri) is actually written**, via
  `spl_token_metadata_interface::instruction::initialize` after
  `InitializeMint2`. Previously only the pointer existed with nothing
  behind it.
- **`DefaultAccountState(Frozen)` is wired into the real mint-creation
  path**, not just exercised in a test as an example of an unsupported
  extension.
- **`transfer_checked_with_fee` + `calculate_epoch_fee` now exist** as an
  actual instruction. Previously there was no fee-charging transfer
  instruction in the program at all.
- **`thaw_after_kyc` now exists.** Previously there was no unfreeze path,
  so nothing could ever leave the frozen-by-default state.
- **PermanentDelegate and confidential transfers are combined on one
  re-issued mint**, with `approve_policy = manual`
  (`auto_approve_new_accounts = false`). Previously they lived in three
  separate, non-overlapping mints, and every confidential mint was hard-
  coded to auto-approve.
- **ConfigureAccount, confidential Transfer, and Withdraw are real program
  instructions**, not just steps a test script ran directly against
  Token-2022. `withdraw_confidential` additionally *enforces on-chain*
  that `ApplyPendingBalance` ran first, by checking
  `pending_balance_credit_counter == 0` via `StateWithExtensions` before
  building the withdraw CPI — previously this was only true by convention
  in test ordering, never checked by the program.

## The seizure/confidentiality gap (task 5's scenario)

`PermanentDelegate` authorizes `transfer_checked` /
`transfer_checked_with_fee` against an account's **public** balance only.
It has no special power over a confidential account's encrypted
available/pending balance — moving that requires a validity/equality/range
proof built from the account owner's own ElGamal secret key, which the
delegate doesn't hold. A sanctioned wallet that keeps its balance
confidential is **not** directly seizable through this authority.

This scaffold doesn't paper over that with a fake "confidential seize"
instruction — there isn't a cryptographically sound way to build one
without the owner's cooperation. Pick one of these before shipping to
mainnet (neither is implemented here, since both are policy calls, not
protocol mechanics):

1. Require confidential accounts to withdraw to the public balance above
   certain thresholds, so the seizure path always has something reachable.
2. Gate `configure_confidential_account` behind the same KYC/allowlist
   check that clears the freeze, so confidentiality is only ever available
   to wallets you've already screened.

## Fix log

**`ProofLocation`/`ProofData` compile errors in `instructions/confidential.rs`
(private enum / not found):** fixed by checking your own already-compiling
test file (`programs/token_22/tests/confidential.rs`) directly rather than
guessing from generic docs.rs pages, which were pulling from a different
`spl-token-2022` version than the one your `Cargo.lock` actually resolves.
Two real bugs, both now fixed:

1. `ProofLocation` is only imported *privately* inside
   `spl_token_2022::extension::confidential_transfer::instruction` — you
   can't reach it as `confidential_instruction::ProofLocation` from outside
   that module. It has to come from its own origin crate:
   `use proofext::instruction::ProofLocation;` (now a real, non-dev
   dependency in `Cargo.toml` — it was only a dev-dependency before, which
   wouldn't have helped since `lib.rs` needs it in program code, not tests).
2. There is no `ProofData` wrapper type at your pinned version —
   `InstructionOffset` takes `(NonZeroI8, &'a T)` directly, confirmed
   against your test's own
   `ProofLocation::InstructionOffset(NonZeroI8::new(1).unwrap(), &proof)`.

Two further corrections that came out of reading the test closely:

- `confidential_transfer` (the program instruction) now takes two more
  params, `ciphertext_lo`/`ciphertext_hi` (`[u8; 64]` each) — the split
  ElGamal ciphertext of the transfer amount that `confidential_transfer::
  instruction::transfer` needs alongside its three proof locations. These
  were missing entirely before.
- `configure_confidential_account` switched from `InstructionOffset` to
  `ProofLocation::ContextStateAccount`. Your test uses `InstructionOffset`
  with a raw `PubkeyValidityProofData` struct reference — fine for a
  client-side test that has the struct in hand, but our *program*
  instruction would need to accept that whole struct as instruction data
  (version-sensitive byte layout, another thing I can't verify compiles
  here). Using a pre-staged context-state account instead — the same
  pattern already used for Transfer/Withdraw — needs only a `Pubkey`, and
  is an equally valid, documented way to satisfy `ConfigureAccount`'s proof
  requirement.

**Still unverified:** `apply_pending_balance`'s exact parameter shape
(pass-by-value vs pass-by-reference for the balance argument) — your test
only exercises this through your own program's wrapper instruction, never
`ct_ix::apply_pending_balance` directly, so I don't have a proven call site
to check it against the way I did for the other four. If `cargo build`
flags a type mismatch there, it's almost certainly just `balance` vs
`&balance`.

## Building

```bash
anchor build
```

**Version note, read before you build:** `instructions/confidential.rs` is
the highest-risk file in this scaffold. The exact function signatures for
`confidential_transfer::instruction::{configure_account, transfer,
withdraw}` (argument order, whether they return `Instruction` or
`Vec<Instruction>`, the exact `ProofLocation`/`ProofData` enum shapes) have
moved across `spl-token-2022` releases, and this sandbox has no Solana/
Anchor toolchain to compile against — I couldn't verify this file builds.
Everything else (`create_mint.rs`, `transfer.rs`, `validate.rs`, `kyc.rs`,
`reissue_mint.rs`, `seize.rs`) mirrors patterns your original repo already
had compiling against `anchor-lang`/`anchor-spl` `1.2.0`, so those are on
much firmer ground. If `confidential.rs` doesn't compile as-is, run
`cargo doc --open -p spl-token-2022` (or check
`~/.cargo/registry/src/.../spl-token-2022-*/src/extension/confidential_transfer/instruction.rs`)
against whatever version your `Cargo.lock` actually resolves, and adjust
the call sites — the account wiring and CPI/invoke structure around them
should still be correct even if an argument needs reordering.

`spl-token-metadata-interface` is declared as a direct dependency at
`"1.0"` in `programs/token_22/Cargo.toml` — it's already pulled in
transitively by anchor-spl's `token_2022_extensions` feature, so if `cargo
build` complains about a version conflict, drop the direct dependency line
and let anchor-spl's copy resolve, or pin to whatever `cargo tree -p
spl-token-metadata-interface` reports.

## Testing

The original repo's test suite (`litesvm` + `spl-token-confidential-
transfer-proof-generation` for client-side proof generation) is the right
model to extend: stand up a mint with `create_remittance_mint_v2`,
generate the pubkey-validity / equality / ciphertext-validity / range
proofs client-side with `proofgen`, stage them as context-state accounts
(or as the next instruction, for `ConfigureAccount`'s instruction-offset
proof), then call the corresponding program instruction. `dev-dependencies`
in `programs/token_22/Cargo.toml` already lists the crates you'll need for
that (`litesvm`, `zk`/`solana-zk-sdk`, `proofgen`, `proofext`, `zkif`,
`token_22new`/`spl-token-2022-interface`).

