# Group Message SEND Path — Full Audit Report

Here is the structured audit based on deep reading of:
- `src/lib.rs` → `introvert_group_send_message` (FFI, lines 2581–2699)
- `src/network/mod.rs` → `BroadcastGroupMessage` handler (lines 2455–2506), gossipsub handler (lines 1394–1420), `GroupInvite` handler (lines 4476–4480), gossipsub subscription at startup (lines 144–153)
- `src/network/group.rs` → `GroupManager::sign_action`, `verify_action`, `wrap_group_secret`
- `src/network/behaviour.rs` → gossipsub config (lines 122–139)

---

## 1. FFI Layer — Plaintext Reception & Encryption

**Entry point:** `introvert_group_send_message` at `lib.rs:2581`

**Plaintext reception:** The FFI receives three raw `*const c_char` pointers: `group_id`, `msg`, and optionally `reply_to`. Null pointer guards are present. The message is converted from C string to Rust `String` via `CStr::to_string_lossy` (no size validation here).

**Encryption flow in the FFI layer:**
```
1. Loads group_secret from DB (load_group_secret) → early return if None or len != 32
2. Generates fresh 12-byte nonce via rand::thread_rng().fill_bytes()
3. Encrypts with AES-256-GCM(group_secret, nonce, plaintext_bytes)
4. Prepends nonce to ciphertext → content_encrypted = [nonce || ciphertext]
5. Signs the GroupAction::Message with sign_action()
6. Stores local copy with is_me=true BEFORE broadcasting
7. Sends ForwardMeshSignaling to each member peer individually (NOT gossipsub publish from FFI)
```

---

## 2. AES-GCM Nonce Freshness ✅

**Yes — nonce is freshly generated for each message.**

`lib.rs:2615–2616`:
```rust
let mut nonce_bytes = [0u8; 12];
rand::thread_rng().fill_bytes(&mut nonce_bytes);
```

This is **correct** behavior. The nonce is 96 bits from `rand::thread_rng()` (cryptographically secure on all supported platforms). No nonce counter or static nonce is used.

**⚠️ IMPORTANT: Duplicate encryption paths exist.** The `BroadcastGroupMessage` command handler in `mod.rs:2477–2484` **also** independently generates a fresh nonce and re-encrypts. This path is triggered for GROUP FILE SHARING manifests (not for regular text messages from the FFI). So there are **two distinct encryption sites**, both using fresh nonces — but this divergence is a maintainability risk.

---

## 3. Group Secret Loading — DB at Send Time ✅

**The secret is loaded from the database at every call, NOT from a cache.**

`lib.rs:2600–2606`:
```rust
let group_secret_vec = match engine.storage.load_group_secret(&group_id) {
    Ok(Some(s)) => { ... s }
    _ => return FfiResult::error(-1, "Group secret not found"),
};
```

This is **correct** from a security standpoint. No in-memory cache means no long-lived secret exposure in RAM. However, it adds a DB round-trip on every send (performance tradeoff).

---

## 4. Zero-Check on Secret ⚠️ PARTIAL BUG

**In the FFI send path (`lib.rs:2600–2611`):** Only a length check is done — no check that the 32 bytes are non-zero:
```rust
if group_secret_vec.len() != 32 {
    return FfiResult::error(-2, "Invalid group secret length");
}
// ← NO ZERO CHECK HERE — all-zero AES key is silently used
```

**In the RECEIVE path (`mod.rs:4516–4519`):** There IS an all-zeros check:
```rust
let is_all_zeros = group_info.secret.iter().all(|&b| b == 0);
if is_all_zeros {
    // triggers a GroupManifestRequest to recover secret
}
```

**BUG:** The send path has no equivalent guard. If a DB corruption or migration bug stores a zeroed secret, the sender will happily encrypt with a null key and broadcast ciphertext that every peer could theoretically decrypt with the same zeroed key. **A `if group_secret.iter().all(|&b| b == 0) { return error }` guard is missing from `lib.rs:2611`.**

---

## 5. Broadcast Mechanism — HYBRID (Dual Path)

**Two separate broadcast mechanisms are used, and they are NOT unified:**

### Path A — FFI text messages (`lib.rs:2667–2693`):
Uses **`ForwardMeshSignaling` (request-response)** sent individually to each member peer:
```rust
for m in members {
    if m.peer_id == my_peer_id_clone { continue; }
    // sends NetworkCommand::ForwardMeshSignaling { peer_id, payload }
}
```
This is **unicast to each member** via the request-response protocol (`/introvert/signaling/2.0.0`), NOT gossipsub.

### Path B — File manifest broadcast (`mod.rs:5392–5396`):
Uses **`BroadcastGroupMessage`** command which ultimately calls `PublishGossipsub`:
```rust
tx.send(NetworkCommand::PublishGossipsub { topic: gid, data }).await
```
This IS gossipsub.

**Implications:**
- Text messages bypass gossipsub entirely — they are direct peer-to-peer sends.
- File manifests use gossipsub.
- If a member is offline, only anchor-mode nodes relay via mailbox for the gossipsub path; the FFI unicast path has no offline delivery fallback visible in this code.

---

## 6. `signed_action` Fields — group_id and signer_peer_id ✅

In `group.rs:21–36`:
```rust
pub fn sign_action(group_id: String, action: GroupAction, keypair: &Keypair) -> Result<SignedGroupAction> {
    let payload = serde_json::to_vec(&(&group_id, &action))?;
    let signature = keypair.sign(&payload)...;
    Ok(SignedGroupAction {
        group_id,               // ← passed in from caller — verified to be the group_id arg
        action,
        signer_peer_id: PeerId::from(keypair.public()).to_string(),  // ← derived from keypair, correct
        signature,
        timestamp: SystemTime::now()...,
    })
}
```

- `group_id` is set correctly from the call site (`lib.rs:2635`): `group_id.clone()` passed in, which comes from the FFI argument validated earlier.
- `signer_peer_id` is derived from `PeerId::from(keypair.public())` — this is cryptographically correct and cannot be spoofed.

✅ No issues here.

---

## 7. Local Copy Storage With is_me=true ✅

**Yes — before broadcasting**, the sender stores their own copy:

`lib.rs:2641–2643`:
```rust
if let Err(e) = engine.storage.store_group_message(&group_id, &my_peer_id, &msg_id, &message, true, reply_to.as_deref()) {
    return FfiResult::error(-5, &format!("Database error: {}", e));
}
```

- `is_me = true` is explicitly passed.
- The plaintext `&message` is stored (not the ciphertext) — correct for local display.
- This happens **synchronously** in the FFI call, before the async broadcast spawn. Good ordering.

---

## 8. Message Size Limits ⚠️ RISK

**Gossipsub:** The `behaviour.rs` gossipsub config (lines 127–133) explicitly has **NO `max_transmit_size`**:
```rust
let gossipsub_config = gossipsub::ConfigBuilder::default()
    .heartbeat_interval(std::time::Duration::from_secs(10))
    .validation_mode(gossipsub::ValidationMode::Strict)
    .message_id_fn(message_id_fn)
    // NOTE: No max_transmit_size — unlimited is the v34/v37 baseline.
    .build()
```

The comment says this is intentional to avoid silently dropping large profile avatars / messages.

**Request-response (FFI text path):** The codec has a **10MB limit**:
```rust
.set_request_size_maximum(10 * 1024 * 1024) // 10MB
.set_response_size_maximum(10 * 1024 * 1024)
```

**There is NO validation of message length at the FFI entry point** (`lib.rs:2581`). A very large text message (e.g., 50MB base64 blob) will:
1. Be encrypted in-memory without a size check.
2. Attempt request-response dispatch — which will be **rejected** at the codec level with a 10MB limit (silently logged, not returned to the caller).
3. The local DB write will have succeeded but the network send will fail.

**Missing: an upfront size guard in `introvert_group_send_message` (e.g., `if message.len() > 65535 { return error }`).**

---

## 9. msg_id Format — Collision Risk ⚠️

**Regular messages:**
```rust
// lib.rs:2626
let mut msg_id = format!("gm_{}_{}", group_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
```

**File messages:** `msg_id` is replaced with `transfer_id` (a UUID from file transfer logic).

**`BroadcastGroupMessage` handler (mod.rs:2487):**
```rust
let mut msg_id = format!("gm_int_{}_{}", gid, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
```

**Issues:**
1. **Timestamp-based, NOT random** — two messages sent in the same nanosecond from the same sender to the same group will have **identical msg_ids**. On modern hardware where nanosecond precision is insufficient (coalesced timer ticks), collisions are possible.
2. `timestamp_nanos_opt()` falls back to `0` on overflow, creating a degenerate collision.
3. The DB likely uses `msg_id` as a unique key — a collision would silently overwrite the first message.

**Additionally:** The gossipsub `message_id_fn` in `behaviour.rs:122–126` hashes `message.data` with `DefaultHasher` (not cryptographically secure, 64-bit):
```rust
let message_id_fn = |message: &gossipsub::Message| {
    let mut s = DefaultHasher::new();
    message.data.hash(&mut s);
    gossipsub::MessageId::from(s.finish().to_string())
};
```
This is used by gossipsub to deduplicate messages in the mesh. Two messages with the same content will be deduped. But more dangerously, a **64-bit hash collision on different content** would cause a legitimate message to be silently dropped. This is a low but non-zero probability risk.

---

## 10. Message Delivery Confirmation ❌ NONE

**There is no delivery acknowledgment mechanism for group messages.**

- The FFI `introvert_group_send_message` returns `FfiResult::success()` immediately after enqueuing the async send (line 2698) — this only means "the task was spawned", not "the message was delivered."
- No ACK, read receipt, or delivery status update is sent back from recipients.
- The only reliability fallback is: **anchor/RBN mailbox** (`StoreInMailbox` command) — used only on the *receive* side when anchor mode is enabled, and only for the gossipsub path.
- The FFI unicast path (ForwardMeshSignaling) has no retry or delivery confirmation.

---

## CRITICAL: gossipsub Subscription Gap for Group Creator ❌ BUG

**This is the most severe finding.**

When `introvert_group_create` is called (`lib.rs:2457–2577`):
1. It saves the group + secret to DB. ✅
2. It sends `GroupInvite` signaling payloads to each member. ✅
3. **It NEVER subscribes to the gossipsub topic for the new group.** ❌

The creator's gossipsub subscription only happens:
- **At startup** (`mod.rs:144–153`): Subscribes to all groups already in the DB. ✅ But only at node startup — not dynamically.
- **On `GroupInvite` received** (`mod.rs:4476–4480`): For invited members.
- **On `GroupManifest` received** (`mod.rs:4867–4873`): For synced members.

**There is no subscribe call after `upsert_group` + `save_group_secret` in `introvert_group_create`.**

**Consequence:** The group creator will NOT receive gossipsub-broadcasted messages in the newly created group during the CURRENT SESSION. They will receive them after the next app restart (when startup re-subscribes from DB).

Since regular text messages use the ForwardMeshSignaling unicast path (not gossipsub), this specifically affects:
- **File manifest broadcasts** (which use `BroadcastGroupMessage → PublishGossipsub`)
- Any other gossipsub-delivered content

The fix is to add, at the end of `introvert_group_create` after saving to DB:
```rust
// Subscribe to gossipsub topic for the newly created group
let _ = tx.send(NetworkCommand::PublishGossipsub { ... }) // No — need a SubscribeGossipsub command
// OR trigger the subscribe directly inside the network event loop via a new NetworkCommand::SubscribeGossipsub { group_id }
```

---

## Summary Table

| # | Check | Status | Severity |
|---|-------|--------|----------|
| 1 | FFI plaintext reception & encryption flow | ✅ Correct | — |
| 2 | Fresh AES-GCM nonce per message | ✅ Yes | — |
| 3 | Secret loaded from DB at send time (not cached) | ✅ Yes | — |
| 4 | Non-zero secret check before encrypting | ⚠️ Missing in send path | Medium |
| 5 | Broadcast mechanism | ⚠️ Dual path (RR unicast for text, gossipsub for files) | Medium |
| 6 | signed_action.group_id / signer_peer_id correct | ✅ Yes | — |
| 7 | Sender stores local copy with is_me=true | ✅ Yes | — |
| 8 | Size limit on messages | ⚠️ No FFI-level guard; gossipsub unlimited; RR 10MB | Medium |
| 9 | msg_id format — deterministic, collision risk | ⚠️ Timestamp-based, can collide; gossipsub uses 64-bit hash | Medium |
| 10 | Message delivery confirmation | ❌ None | High |
| 11 | Creator subscribes to gossipsub on group create | ❌ Missing — creator blind to gossipsub traffic until restart | High |

---

## Top 3 Actionable Bugs

### Bug A (High): Creator not subscribed to gossipsub after group creation
**File:** `src/lib.rs` around line 2532–2574  
**Fix:** After `save_group_secret` succeeds, send a new `NetworkCommand::SubscribeGossipsub { group_id }` to the network event loop (needs a new command variant). In the command handler, call `self.swarm.behaviour_mut().gossipsub.subscribe(&topic)`.

### Bug B (Medium): No all-zeros guard on group secret at send time
**File:** `src/lib.rs` line 2611 (after `copy_from_slice`)  
**Fix:** Add:
```rust
if group_secret.iter().all(|&b| b == 0) {
    return FfiResult::error(-6, "Group secret is all-zeros — refusing to encrypt");
}
```

### Bug C (Medium): Timestamp-based msg_id is non-unique
**File:** `src/lib.rs` line 2626  
**Fix:** Use a UUID or `format!("gm_{}_{}_{}", group_id, timestamp_ns, rand::random::<u32>())` to ensure uniqueness:
```rust
let msg_id = format!("gm_{}_{}_{:08x}", group_id, 
    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    rand::random::<u32>());
```
