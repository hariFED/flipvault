# Arcium 0.11.1 — Persisted Encrypted-State Patterns (FlipVault Path-B reference)

Verbatim patterns extracted from `github.com/arcium-hq/examples` branch **`v0.11.1`** (voting,
sealed_bid_auction, blackjack) + `docs.arcium.com/*.md`. Our stack: arcium CLI 0.11.2, crates
`arcium-anchor/client/macros = 0.11.1`, `arcis = 0.11.1`, `anchor-lang 1.0.2`, `@arcium-hq/client 0.11.1`.

> The example repo's `main` branch is pinned to **0.9.6** (6-arg `queue_computation`, no `#[args]`).
> Always read the **`v0.11.1`** branch. Two 0.11 differences that bite: `queue_computation` takes a
> 7th arg `callback_cu_limit: u32` (pass `0`); and the `let args = ArgBuilder::new()…` binding
> carries an `#[args("<ix_name>")]` attribute.

## 1. Storing ciphertext in accounts
No `MXEEncryptedStruct` account type exists. Store raw `[[u8;32]; N]` + a `u128` nonce. `N` = field-element
count the circuit serializes to (read it from the generated `*Output` type after `arcium build`).
For `Shared`, also store the owner's x25519 key as a separate `[u8;32]`.

```rust
#[account]
#[derive(InitSpace)]
pub struct PollAccount {            // Enc<Mxe, VoteStats{yes,no}>  → N=2
    pub bump: u8,
    pub vote_state: [[u8; 32]; 2],  // ciphertext at offset 8+1 = 9
    pub nonce: u128,
    // ...
}
```
Keep `bump` first and the ciphertext array second so it sits at a known offset (9) for `.account(...)`.

## 2. Feeding persisted account ciphertext INTO a computation
`ArgBuilder::new()` then, **in circuit-parameter order**:
- `Enc<Mxe,T>`  → `.plaintext_u128(nonce).account(key, offset, 32*N)`
- `Enc<Shared,T>` → `.x25519_pubkey(stored_key).plaintext_u128(nonce).account(key, offset, 32*N)`
- public scalar → `.plaintext_u128(v)` (also `plaintext_u8/u64/...`)
- fresh ciphertext arg (not persisted) → `.encrypted_u8/u16/u32/u64/u128/bool([u8;32])` (an encrypted **bool** is `.encrypted_bool(ct)`)
The binding needs `#[args("ix_name")]`. Offsets computed `8 + sum(preceding fields)`; keep encrypted blobs contiguous.

```rust
#[args("vote")]
let args = ArgBuilder::new()
    .x25519_pubkey(vote_encryption_pubkey)
    .plaintext_u128(vote_nonce)
    .encrypted_bool(vote)                                  // fresh Enc<Shared,bool> arg
    .plaintext_u128(ctx.accounts.poll_acc.nonce)
    .account(ctx.accounts.poll_acc.key(), 8 + 1, 32 * 2)   // persisted Enc<Mxe,VoteStats>
    .build();

queue_computation(
    ctx.accounts, computation_offset, args,
    vec![VoteCallback::callback_ix(
        computation_offset, &ctx.accounts.mxe_account,
        &[CallbackAccount { pubkey: ctx.accounts.poll_acc.key(), is_writable: true }],
    )?],
    1, 0, 0,                                                // num_callback_txs, cu_price, callback_cu_limit
)?;
```
The custom `CallbackAccount` slice order MUST match the custom-account order in the callback `#[derive(Accounts)]`.

## 3. Writing the callback output back
Single output: `Ok(IxOutput { field_0 }) => field_0`, then `.ciphertexts` / `.nonce` (+ `.encryption_key` for Shared).
Tuple output: nests as `IxOutput { field_0: IxOutputStruct0 { field_0, field_1, ... } }`; a revealed `bool` element is just a `bool`.

```rust
let (deck, dealer, player, ok) = match output.verify_output(
    &ctx.accounts.cluster_account, &ctx.accounts.computation_account) {
    Ok(IxOutput { field_0: IxOutputStruct0 { field_0, field_1, field_2, field_3 } })
        => (field_0, field_1, field_2, field_3),
    Err(_) => return Err(ErrorCode::AbortedComputation.into()),
};
acct.deck       = deck.ciphertexts;     acct.deck_nonce   = deck.nonce;
acct.player_hand= player.ciphertexts[0];acct.client_nonce = player.nonce;
acct.player_enc_pubkey = player.encryption_key;   // Shared output's x25519 key
// `ok` is a plain bool when the circuit returned a revealed value
```

## 4. Nonce lifecycle
The **cluster** mints the new nonce when it re-encrypts an output; it arrives as `o.<field>.nonce`. The program
stores it and replays it via `.plaintext_u128(stored_nonce)` next time. The **client** only generates nonces for
fresh inputs it encrypts itself (`randomBytes(16)`). Never increment a persisted nonce manually.

## 5. Genesis of encrypted state
`Enc<Mxe,T>` cannot be produced by the client — mint it with a tiny circuit `Mxe::get().from_arcis(<value>)`, queued
with empty/plaintext args, callback persists `o.ciphertexts`/`o.nonce`:
```rust
#[instruction] pub fn init_vote_stats() -> Enc<Mxe, VoteStats> { Mxe::get().from_arcis(VoteStats{yes:0,no:0}) }
```
`Enc<Shared,T>` genesis CAN be produced client-side (the client holds the shared secret) and passed to a register
instruction as `ct + enc_pubkey + nonce` — no circuit needed. (Path-B uses this for boxes; `init_curve`/`init_treasury`
circuits for the Mxe curve+treasury.)

## 6. TS client (encrypt in / decrypt own Shared state) — `@arcium-hq/client`
```ts
import { RescueCipher, x25519, getMXEPublicKey, deserializeLE, awaitComputationFinalization } from "@arcium-hq/client";
import { randomBytes } from "crypto";

const mxePublicKey = await getMXEPublicKeyWithRetry(provider, program.programId); // retry: key appears post-deploy
const priv = x25519.utils.randomSecretKey();
const pub  = x25519.getPublicKey(priv);
const shared = x25519.getSharedSecret(priv, mxePublicKey);
const cipher = new RescueCipher(shared);

const nonce = randomBytes(16);
const ct = cipher.encrypt([sol, perp, inPerp, costBasis].map(BigInt), nonce); // -> [u8;32][]
// pass Array.from(ct[i]), Array.from(pub), new anchor.BN(deserializeLE(nonce).toString())

// decrypt own box after a flip: nonce comes from the account/event (cluster-assigned)
const boxNonce = Uint8Array.from(boxAcct.nonce.toArray("le", 16));
const [sol, perp, inPerp, cb] = cipher.decrypt([c0,c1,c2,c3], boxNonce); // BigInt[]
```
Derive the player x25519 key deterministically from the wallet (sign a fixed message → sha256) so the same key
that `enc_pubkey` was sealed to is always recoverable.

## Open items (resolve from generated artifacts)
- Exact `N` per struct: read the `*Output` type / `build/*.idarc` after `arcium build` (our IDL: Curve=2, treasury=1, BoxState=4).
- `*OutputStruct0` nesting depth for tuples: mirror the generated type exactly.
- `#[args]` is in v0.11.1 examples but absent from some prose docs — trust the example branch.
