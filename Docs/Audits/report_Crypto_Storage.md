# Deep Security Audit Report — Introvert P2P Group Chat System

I have completed a full read of all three files and traced all call sites. Here is the structured audit report.

---

## Audit Scope Summary

| File | Lines | Focus |
|---|---|---|
| `src/network/group.rs` | 215 | GroupManager: sign, verify, wrap/unwrap secret, TreeKEM |
| `src/network/types.rs` | 359 | SignalingPayload enum, GroupAction enum |
| `src/storage.rs` | 2609 | All group storage functions |
| `src/lib.rs` | 4686 (partial) | introvert_group_create — secret generation |
| `src/network/mod.rs` | 5524 (partial) | AcceptGroupInvite, GroupInvite receive handler, ApproveGroupJoin |

---

## Key Questions — Direct Answers

### 1. Secret generation: is the secret 32 bytes of cryptographically random data?
**YES — but with a caveat.** In `src/lib.rs:2477`:
```rust
let secret = rand::random::<[u8; 32]>();
```
`rand::random` uses `ThreadRng`, which is cryptographically secure (it seeds from the OS via `getrandom`). This is acceptable, but the industry-standard pattern for secrets is `OsRng` explicitly. `ThreadRng` is CSPRNG-backed but has additional user-space state, which introduces a marginally larger attack surface. This is LOW severity but worth noting.

### 2. Does `upsert_group` use ON CONFLICT DO UPDATE without touching the secret column?
**CONFIRMED SAFE.** `upsert_group` (`storage.rs:1267-1275`) only touches the `groups` table (`name`, `description`, `members_json`). The secret lives in a **separate** `group_secrets` table (`storage.rs:260-263`). The two tables are completely decoupled, so `upsert_group` **cannot accidentally reset the secret**. ✅

### 3. Is there a separate `group_secrets` table or is secret stored in the `groups` table?
**SEPARATE TABLE.** Schema at `storage.rs:260-263`:
```sql
CREATE TABLE IF NOT EXISTS group_secrets (
    group_id TEXT PRIMARY KEY,
    secret_blob BLOB NOT NULL
);
```
The `groups` table has no secret column. This is the correct design. ✅

### 4. What happens if `store_pending_invite` is called twice for the same `group_id`?
**It silently OVERWRITES.** `storage.rs:1461-1468`:
```sql
INSERT INTO pending_group_invites (...) VALUES (...)
ON CONFLICT(group_id) DO UPDATE SET name = excluded.name,
    description = excluded.description,
    inviter_peer_id = excluded.inviter_peer_id,
    group_secret_wrapped = excluded.group_secret_wrapped,
    members_json = excluded.members_json
```
A second invite from a **different attacker peer** for the same `group_id` silently replaces the `group_secret_wrapped` and `inviter_peer_id`. If the user later accepts, they unwrap **the attacker's wrapped ciphertext**, not the real admin's. This is a **CRITICAL** issue — see finding #1.

### 5. Is there a race condition risk of `upsert_group` AFTER `save_group_secret`?
**NO RACE POSSIBLE** because they touch different tables (`groups` vs `group_secrets`). `upsert_group` cannot touch `secret_blob`. Additionally, the `parking_lot::Mutex<Connection>` serialises all DB access to a single connection. ✅

### 6. Are all SQL operations on the same connection/mutex so there are no partial writes?
**YES — single mutex, single connection.** `StorageService` wraps one `Mutex<Connection>` (`storage.rs:34`). All methods acquire the same mutex before executing. No partial writes are possible across methods **as long as they do not need to be atomic with each other** (see finding #5 about the `get_group` lock release gap). ✅ (with caveat)

### 7. Is `group_secret` ever serialised into `SignalingPayload::GroupManifest`?
**NO — the secret is NOT in `GroupManifest`.** `types.rs:159`:
```rust
GroupManifest { group_id: String, name: String, description: String, members: Vec<GroupMemberMetadata> },
```
No secret field. The comment in `mod.rs:4880-4881` also explicitly states: *"Do NOT save secret from manifest — it's no longer transmitted in plaintext."* ✅

The secret is only transmitted as `group_secret_wrapped` inside `GroupInvite` (`types.rs:157`), which uses ECDH + AES-GCM wrapping per-recipient. ✅

---

## Findings

---

### FINDING #1 — Pending Invite Poisoning / TOCTOU Secret Swap
**Severity: CRITICAL**
**File:** `src/storage.rs:1459-1469`, `src/network/mod.rs:4453-4491`
**Lines:** storage.rs:1461-1468 (ON CONFLICT DO UPDATE), mod.rs:4491 (store_pending_invite callsite)

**Description:**
`store_pending_invite` uses `ON CONFLICT DO UPDATE` which silently replaces the `group_secret_wrapped` blob if the same `group_id` arrives a second time. A malicious peer who knows a valid `group_id` can send a spoofed `GroupInvite` message with a `group_secret_wrapped` value that wraps **an attacker-controlled key** instead of the real group secret. When the victim calls `AcceptGroupInvite`, they decrypt the attacker's blob and save a wrong/controlled key as their group secret — meaning they can no longer decrypt real group messages, or the attacker learns whatever they fed in.

**Root Cause:**
No authentication check is performed on inbound `GroupInvite` messages before persisting them. `inviter_peer_id` is a String field populated from the network message itself — anyone can claim to be the inviter.

**Suggested Fix:**
1. Before storing, verify that `inviter_peer_id` is already a known contact AND is actually a member/admin of `group_id` in an already-stored group (if one exists). If not, drop the invite.
2. Add an HMAC or signature over the wrapped payload using the inviter's p2p keypair. Reject unsigned or badly-signed invites.
3. Change `ON CONFLICT DO UPDATE` to `INSERT OR IGNORE` so a second invite for the same `group_id` is silently dropped after the first. Alert the user if an already-pending invite arrives from a different inviter.

---

### FINDING #2 — Missing `pending_group_invites` Table DDL in `bootstrap()`
**Severity: CRITICAL**
**File:** `src/storage.rs:131-413` (entire `bootstrap()` function)

**Description:**
The `pending_group_invites` table is **never created in `bootstrap()`**. Searching the entire `storage.rs` file, the string `"CREATE TABLE"` does not appear with `pending_group_invites` anywhere. Yet `store_pending_invite`, `get_pending_invite`, `get_pending_invites`, and `delete_pending_invite` all issue SQL against this table. On a fresh install, all these operations will fail with `rusqlite::Error::SqliteFailure` ("no such table: pending_group_invites"). Because return values are discarded with `let _ = ...` at the call sites in `mod.rs:4491`, this failure is **silently swallowed**.

The practical result: on a fresh install, `store_pending_invite` fails silently, `get_pending_invite` fails silently, and when the user taps "Accept Invite", `AcceptGroupInvite` finds no pending invite and emits `"[Mesh] No pending invite found for group"` — the user can never join any group by invite.

**Root Cause:**
The table DDL was omitted from `bootstrap()`'s `execute_batch` call and was not added as a migration either.

**Suggested Fix:**
Add the following to the `execute_batch` string in `bootstrap()`:
```sql
CREATE TABLE IF NOT EXISTS pending_group_invites (
    group_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    inviter_peer_id TEXT NOT NULL,
    group_secret_wrapped BLOB NOT NULL,
    members_json TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```
Also add a migration line with `CREATE TABLE IF NOT EXISTS pending_group_invites (...)` to handle existing databases that already have the `groups` and `group_secrets` tables.

---

### FINDING #3 — `get_group` Releases Lock Between Two Reads (Non-Atomic Join)
**Severity: HIGH**
**File:** `src/storage.rs:1331-1364`
**Lines:** 1332-1341 (conn lock acquired and dropped), 1344 (load_group_secret acquires new lock)

**Description:**
`get_group` reads from `groups` inside a scoped lock block, releases the mutex, then calls `self.load_group_secret()` which acquires the mutex again. Between the two lock acquisitions, another thread could call `save_group_secret` or `delete_group`. In a rare but possible scenario on a multi-threaded async runtime, the `groups` row exists when read but the secret is deleted before `load_group_secret` returns — resulting in `get_group` returning a `GroupMeshInfo` with an all-zero secret `[0u8;32]`.

**Root Cause:**
The two-table design (correct for isolation) requires a join-like operation that needs atomicity. The implementation breaks that atomicity by releasing the connection mutex in between.

**Suggested Fix:**
Execute both queries within a single lock acquisition:
```rust
pub fn get_group(&self, group_id: &str) -> Result<Option<GroupMeshInfo>> {
    let conn = self.conn.lock();
    let mut stmt = conn.prepare("SELECT g.group_id, g.name, g.members_json, g.description, gs.secret_blob FROM groups g LEFT JOIN group_secrets gs ON g.group_id = gs.group_id WHERE g.group_id = ?1")?;
    // ... single query, no lock release in between
}
```

---

### FINDING #4 — `verify_action` Does Not Check the Timestamp / Replay Attack
**Severity: HIGH**
**File:** `src/network/group.rs:38-74`
**Lines:** 38-74 (`verify_action` entire body)

**Description:**
`SignedGroupAction` has a `timestamp: u64` field (`types.rs:72`). `verify_action` never checks this timestamp. A captured, signed `GroupAction` (e.g., `AddMember`, `UpdateRole`, `DeleteMessage`) can be replayed indefinitely at any future point. An attacker who captures a signed `UpdateRole` action elevating peer X can replay it at will to re-grant elevated roles even after they've been revoked, or replay `DeleteGroup` after the group is re-created.

**Root Cause:**
`verify_action` only validates membership and cryptographic signature. Temporal validity is not enforced.

**Suggested Fix:**
1. In `verify_action`, reject actions where `signed.timestamp` is more than N seconds in the past (e.g., 300 seconds).
2. Maintain a per-group seen-nonce set or use a monotonic sequence number per member rather than a raw timestamp.

---

### FINDING #5 — `EditMessage` / `DeleteMessage` Ownership Not Verified
**Severity: HIGH**
**File:** `src/network/group.rs:57-58`
**Lines:** 57-58

**Description:**
The inline comments literally read: *"Allowed (verifying ownership is handled later/implicitly)"*. Looking at the `GroupAction` handler in `mod.rs`, ownership verification for `EditMessage` and `DeleteMessage` is **never actually implemented**. Any `Member`-role peer can edit or delete any other member's message.

**Root Cause:**
The permission check for `EditMessage` and `DeleteMessage` was deferred with a comment and never implemented.

**Suggested Fix:**
In `verify_action` for `EditMessage { msg_id }` and `DeleteMessage { msg_id }`, the system must look up the original message author and confirm `signed.signer_peer_id == message_author`. This requires passing message storage or a message-author lookup callback into `verify_action`, or performing the check in the `GroupAction` handler in `mod.rs` before applying the action.

---

### FINDING #6 — `save_group_secret` Accepts Arbitrary-Length Byte Slices Without Length Validation
**Severity: MEDIUM**
**File:** `src/storage.rs:1401-1409`
**Lines:** 1401-1409

**Description:**
`save_group_secret` accepts `secret: &[u8]` without enforcing that it is exactly 32 bytes. `load_group_secret` returns a raw `Vec<u8>`. The caller in `get_group` (`storage.rs:1348-1352`) does have a length check, but it silently falls back to an all-zero secret if the length is wrong — rather than returning an error. If a future code path calls `save_group_secret` with the wrong-length buffer (e.g., a decoded hex string of incorrect length), the stored secret will be silently truncated to zeros on load.

**Root Cause:**
No enforced invariant at the storage boundary.

**Suggested Fix:**
Change the signature to `secret: &[u8; 32]` (fixed-size reference) and update all callers. The `get_group` fallback to zeros should be changed to `return Err(...)` rather than silently continuing.

---

### FINDING #7 — `store_pending_invite` Silently Stores Encrypted Wrapped Secret from Unverified Source
**Severity: MEDIUM**
**File:** `src/network/mod.rs:4483-4491`
**Lines:** 4453-4491

**Description:**
On receipt of `SignalingPayload::GroupInvite`, the `group_secret_wrapped` blob is stored directly into `pending_group_invites.group_secret_wrapped` without any attempt to verify the ECDH ciphertext's authenticity. There is no MAC over the whole `GroupInvite` message body tied to the inviter's identity keypair. An active network adversary can flip bits in `group_secret_wrapped` causing `unwrap_group_secret` to silently fail with a decryption error — making the invite permanently unacceptable.

**Root Cause:**
The ECDH + AES-GCM wrapping in `wrap_group_secret` provides integrity of the *ciphertext* (GCM tag), but there is no binding to the inviter's peer identity at the envelope level. The inviter could be anyone.

**Suggested Fix:**
Sign the `GroupInvite` payload with the inviter's libp2p keypair (same mechanism as `SignedGroupAction`) before sending, and verify that signature before storing.

---

### FINDING #8 — `rand::random` Used Instead of Explicit `OsRng` for Group Secret
**Severity: LOW**
**File:** `src/lib.rs:2477`
**Line:** 2477

**Description:**
```rust
let secret = rand::random::<[u8; 32]>();
```
`rand::random` relies on `ThreadRng`, which is a CSPRNG seeded from the OS. It is technically cryptographically secure but maintains thread-local state. The explicit recommended pattern for generating cryptographic key material is `OsRng` directly, which draws directly from the kernel entropy pool with no user-space buffering:
```rust
use rand::RngCore;
let mut secret = [0u8; 32];
rand::rngs::OsRng.fill_bytes(&mut secret);
```

**Root Cause:**
Convenience API used where the explicit secure API should be preferred.

**Suggested Fix:**
Replace with `rand::rngs::OsRng.fill_bytes(&mut secret)`.

---

### FINDING #9 — `GroupManifest` Sent as Fallback When No Static Key Available — No Secret Delivery
**Severity: LOW (Design Gap)**
**File:** `src/network/mod.rs:2212-2220`
**Lines:** 2212-2220

**Description:**
When an admin approves a join request but cannot find the requester's static key to wrap the group secret, a `GroupManifest` is sent as a fallback (line 2214-2220). The `GroupManifest` does not contain the group secret (`types.rs:159`). This means the newly joined member has the group row in their DB (via `upsert_group` on `GroupManifest` receive at mod.rs:4883) but has **no secret** — their `get_group` returns an all-zero secret. They can see the group exists but cannot decrypt any messages. This is a silent failure with no error surfaced to the user.

**Root Cause:**
The fallback path delivers group metadata but not the secret. The code comment at `mod.rs:2213` acknowledges the situation but does not handle it.

**Suggested Fix:**
1. Do not send `GroupManifest` as a fallback. Instead, queue the approval and wait for a `Handshake` from the requester that carries their static key (similar to the `pending_requester_static_keys` map already in use).
2. Or: in the `GroupManifest` receive handler, if the local secret is zero-bytes after `upsert_group`, immediately send a `GroupManifestRequest` back to probe for a `GroupInvite`.

---

## Summary Table

| # | Severity | File | Lines | Title |
|---|---|---|---|---|
| 1 | **CRITICAL** | storage.rs / mod.rs | 1461-1468 / 4491 | Pending invite poisoning via secret swap |
| 2 | **CRITICAL** | storage.rs | 131-413 | `pending_group_invites` table never created in bootstrap |
| 3 | **HIGH** | storage.rs | 1331-1364 | `get_group` non-atomic dual-lock (TOCTOU) |
| 4 | **HIGH** | network/group.rs | 38-74 | No timestamp/replay check in `verify_action` |
| 5 | **HIGH** | network/group.rs | 57-58 | EditMessage/DeleteMessage ownership never enforced |
| 6 | **MEDIUM** | storage.rs | 1401-1409 | `save_group_secret` accepts unchecked-length slice |
| 7 | **MEDIUM** | network/mod.rs | 4453-4491 | No envelope signature on GroupInvite |
| 8 | **LOW** | lib.rs | 2477 | `rand::random` vs explicit `OsRng` for key material |
| 9 | **LOW** | network/mod.rs | 2212-2220 | GroupManifest fallback delivers group without secret |

---

## Positive Observations
- ✅ Group secret is correctly stored in a **separate table** (`group_secrets`), fully isolated from `upsert_group`
- ✅ `upsert_group` ON CONFLICT touches only `name`, `description`, `members_json` — never the secret
- ✅ `GroupManifest` does **not** include the group secret — correct
- ✅ Secret is wrapped per-recipient using ECDH (X25519) + HKDF + AES-256-GCM — correct design
- ✅ Single `parking_lot::Mutex<Connection>` serialises all DB access — no concurrent write corruption
- ✅ `delete_group` correctly removes from both `groups` AND `group_secrets` tables (`storage.rs:1427-1439`)
- ✅ SQLCipher is used for at-rest encryption of the entire database including the `group_secrets` table