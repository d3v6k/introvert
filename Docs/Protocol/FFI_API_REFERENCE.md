# FFI API Reference

## Overview

The FFI (Foreign Function Interface) bridge connects Flutter (Dart) to the Rust core engine. All functions are exported as `extern "C"` and callable from Dart via `dart:ffi`.

## Core Types

### FfiResult
```rust
#[repr(C)]
pub struct FfiResult {
    pub code: i32,      // 0 = success, non-zero = error
    pub data: *mut u8,  // Heap-allocated data (Dart must free)
    pub len: usize,     // Length of data
}
```

### FfiCallback
```rust
pub type FfiCallback = extern "C" fn(FfiResult);
pub type FfiRewardCallback = extern "C" fn(i32, *const c_char);
pub type FfiNetworkCallback = extern "C" fn(i32, *const u8, usize);
```

## Engine Lifecycle

### introvert_engine_start
```rust
#[no_mangle]
pub extern "C" fn introvert_engine_start(
    seed: *const u8,      // 32-byte master seed
    db_path: *const c_char // Path to SQLCipher database
) -> FfiResult
```
Initializes the engine with identity derivation and database setup.

### introvert_engine_stop
```rust
#[no_mangle]
pub extern "C" fn introvert_engine_stop() -> FfiResult
```
Stops the engine and cleans up resources.

### introvert_get_peer_id
```rust
#[no_mangle]
pub extern "C" fn introvert_get_peer_id() -> *mut c_char
```
Returns the node's PeerId as a null-terminated string.

## Network Operations

### introvert_network_start_production
```rust
#[no_mangle]
pub extern "C" fn introvert_network_start_production(
    callback: FfiNetworkCallback,
    port: u16,
    relay: bool,
    max_connections: u32,
    liveness_check: u64
) -> FfiResult
```
Starts the libp2p swarm with the specified configuration.

### introvert_network_send_message
```rust
#[no_mangle]
pub extern "C" fn introvert_network_send_message(
    peer_id: *const c_char,
    msg: *const c_char,
    reply_to: *const c_char,
    callback: FfiCallback
) -> FfiResult
```
Sends an encrypted message to a peer.

### introvert_network_establish_secure_session
```rust
#[no_mangle]
pub extern "C" fn introvert_network_establish_secure_session(
    peer_id: *const c_char
) -> FfiResult
```
Establishes a Noise IK session with a peer.

### introvert_network_fetch_mailbox
```rust
#[no_mangle]
pub extern "C" fn introvert_network_fetch_mailbox() -> FfiResult
```
Fetches pending messages from RBN mailbox.

## File Operations

### introvert_file_start_transfer
```rust
#[no_mangle]
pub extern "C" fn introvert_file_start_transfer(
    peer_id: *const c_char,
    file_path: *const c_char
) -> FfiResult
```
Initiates a file transfer to a peer.

### introvert_file_get_progress
```rust
#[no_mangle]
pub extern "C" fn introvert_file_get_progress(
    file_id: *const c_char
) -> FfiResult
```
Returns JSON with transfer progress.

### introvert_file_compute_hash
```rust
#[no_mangle]
pub extern "C" fn introvert_file_compute_hash(
    file_path: *const c_char
) -> FfiResult
```
Computes SHA-256 hash of a file.

## Storage Operations

### introvert_storage_get_messages
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_get_messages(
    peer_id: *const c_char
) -> FfiResult
```
Returns JSON array of messages for a peer.

### introvert_storage_store_message
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_store_message(
    peer_id: *const c_char,
    msg: *const c_char,
    is_me: bool,
    callback: FfiCallback
) -> FfiResult
```
Stores a message in the local database.

### introvert_storage_get_contacts
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_get_contacts() -> FfiResult
```
Returns JSON array of all contacts.

### introvert_storage_add_contact
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_add_contact(
    peer_id: *const c_char,
    name: *const c_char,
    handle: *const c_char,
    avatar: *const c_char
) -> FfiResult
```
Adds a new contact to the database.

### introvert_storage_delete_contact
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_delete_contact(
    peer_id: *const c_char
) -> FfiResult
```
Deletes a contact from the database.

## Group Operations

### introvert_group_create
```rust
#[no_mangle]
pub extern "C" fn introvert_group_create(
    name: *const c_char,
    members_json: *const c_char
) -> FfiResult
```
Creates a new group with the specified members.

### introvert_group_send_message
```rust
#[no_mangle]
pub extern "C" fn introvert_group_send_message(
    group_id: *const c_char,
    content: *const c_char
) -> FfiResult
```
Sends a message to a group.

### introvert_group_get_messages
```rust
#[no_mangle]
pub extern "C" fn introvert_group_get_messages(
    group_id: *const c_char
) -> FfiResult
```
Returns JSON array of group messages.

### introvert_group_add_member
```rust
#[no_mangle]
pub extern "C" fn introvert_group_add_member(
    group_id: *const c_char,
    peer_id: *const c_char
) -> FfiResult
```
Adds a member to a group.

### introvert_group_remove_member
```rust
#[no_mangle]
pub extern "C" fn introvert_group_remove_member(
    group_id: *const c_char,
    peer_id: *const c_char
) -> FfiResult
```
Removes a member from a group.

## Wormhole Operations

### introvert_wormhole_start
```rust
#[no_mangle]
pub extern "C" fn introvert_wormhole_start() -> FfiResult
```
Creates a new Wormhole invite (returns 2-word code).

### introvert_wormhole_join
```rust
#[no_mangle]
pub extern "C" fn introvert_wormhole_join(
    code: *const c_char
) -> FfiResult
```
Joins an existing Wormhole invite.

### introvert_wormhole_abort
```rust
#[no_mangle]
pub extern "C" fn introvert_wormhole_abort() -> FfiResult
```
Aborts an active Wormhole operation.

## Economy Operations

### introvert_economy_start_monitoring
```rust
#[no_mangle]
pub extern "C" fn introvert_economy_start_monitoring(
    callback: FfiNetworkCallback
) -> FfiResult
```
Starts monitoring relayed bytes for rewards.

### introvert_economy_claim_rewards_async
```rust
#[no_mangle]
pub extern "C" fn introvert_economy_claim_rewards_async(
    callback: FfiRewardCallback
) -> FfiResult
```
Claims accumulated rewards (async, returns transaction signature).

## Reaction Operations

### introvert_network_send_reaction
```rust
#[no_mangle]
pub extern "C" fn introvert_network_send_reaction(
    target_id: *const c_char,
    msg_id: *const c_char,
    emoji: *const c_char,
    is_group: bool
) -> FfiResult
```
Sends an emoji reaction to a message.

### introvert_storage_get_reactions
```rust
#[no_mangle]
pub extern "C" fn introvert_storage_get_reactions(
    msg_id: *const c_char
) -> FfiResult
```
Returns JSON array of reactions for a message.

## Handle Operations

### introvert_network_claim_handle
```rust
#[no_mangle]
pub extern "C" fn introvert_network_claim_handle(
    handle: *const c_char
) -> FfiResult
```
Claims a handle via PoW consensus.

### introvert_network_resolve_handle
```rust
#[no_mangle]
pub extern "C" fn introvert_network_resolve_handle(
    handle: *const c_char
) -> FfiResult
```
Resolves a handle to a PeerId.

## Message Operations

### introvert_network_delete_message
```rust
#[no_mangle]
pub extern "C" fn introvert_network_delete_message(
    target_id: *const c_char,
    msg_id: *const c_char,
    is_group: bool,
    deleted_by_admin: bool
) -> FfiResult
```
Deletes a message (broadcasts to mesh).

### introvert_network_edit_message
```rust
#[no_mangle]
pub extern "C" fn introvert_network_edit_message(
    target_id: *const c_char,
    msg_id: *const c_char,
    new_content: *const c_char,
    is_group: bool
) -> FfiResult
```
Edits a message (broadcasts to mesh).

## Automation Operations (Intro-Claw)

### intro_claw_get_ai_mode
```rust
#[no_mangle]
pub extern "C" fn intro_claw_get_ai_mode() -> i32
```
Returns the current AI engine mode: `0` = 100% Offline, `1` = Hybrid AI Assistant.

### intro_claw_set_ai_mode
```rust
#[no_mangle]
pub extern "C" fn intro_claw_set_ai_mode(mode: i32, api_key: *const c_char) -> FfiResult
```
Sets the AI mode and optionally stores an encrypted API key for Hybrid mode.

### intro_claw_get_api_key
```rust
#[no_mangle]
pub extern "C" fn intro_claw_get_api_key() -> *mut c_char
```
Returns the stored (encrypted) API key.

### intro_claw_trigger_tick
```rust
#[no_mangle]
pub extern "C" fn intro_claw_trigger_tick() -> FfiResult
```
Manually triggers the intro-claw maintenance tick cycle.

### intro_claw_set_active
```rust
#[no_mangle]
pub extern "C" fn intro_claw_set_active(active: bool) -> FfiResult
```
Enables or disables the intro-claw engine. Persists to `economy_meta` table.

### intro_claw_get_status
```rust
#[no_mangle]
pub extern "C" fn intro_claw_get_status() -> FfiResult
```
Returns JSON status: `{ "ai_mode": 0/1, "api_key_set": bool, "is_active": bool }`.

### intro_claw_get_endpoint
```rust
#[no_mangle]
pub extern "C" fn intro_claw_get_endpoint() -> *mut c_char
```
Returns the stored LLM endpoint URL.

### intro_claw_set_endpoint
```rust
#[no_mangle]
pub extern "C" fn intro_claw_set_endpoint(endpoint: *const c_char) -> FfiResult
```
Stores the LLM endpoint URL for Hybrid mode.

### intro_claw_process_query
```rust
#[no_mangle]
pub extern "C" fn intro_claw_process_query(query: *const c_char) -> FfiResult
```
Processes a natural language query through the assistant engine. Returns JSON with answer, search results, and count.

### intro_claw_run_network_recon
```rust
#[no_mangle]
pub extern "C" fn intro_claw_run_network_recon() -> FfiResult
```
Runs network reconnaissance and returns a monospaced markdown report containing mesh overview, storage usage, peer routing table, connection analysis, upgrade candidates, and security audit.

### intro_claw_heal_peer
```rust
#[no_mangle]
pub extern "C" fn intro_claw_heal_peer(peer_id: *const c_char) -> FfiResult
```
Attempts multi-strategy connection recovery for a specific peer: direct dial → relay circuit → anchor routing → WebSocket tunnel → mailbox fallback. Returns heal report as markdown.

## Utility Functions

### introvert_free_string
```rust
#[no_mangle]
pub extern "C" fn introvert_free_string(s: *mut c_char)
```
Frees a string allocated by Rust.

### introvert_free_binary
```rust
#[no_mangle]
pub extern "C" fn introvert_free_binary(ptr: *mut u8, len: usize)
```
Frees binary data allocated by Rust.

### introvert_generate_mnemonic
```rust
#[no_mangle]
pub extern "C" fn introvert_generate_mnemonic() -> *mut c_char
```
Generates a new 24-word BIP39 mnemonic.

### introvert_mnemonic_to_seed
```rust
#[no_mangle]
pub extern "C" fn introvert_mnemonic_to_seed(
    phrase: *const c_char
) -> FfiResult
```
Converts a mnemonic to a 32-byte seed.

## Event Codes

| Code | Event | Payload |
|------|-------|---------|
| 0 | WebRTC Renegotiation | Literal bytes |
| 1 | Peer Discovered | Binary PeerId |
| 2 | Message Received | Binary message data |
| 4 | Mailbox Drained | Binary message data |
| 5 | Media Frame | Header + payload |
| 6 | Wormhole Invite | UTF-8 code |
| 7 | Handover Complete | UTF-8 PeerId |
| 8 | Peer Status | PeerId + status byte |
| 9 | Economy Stats | UTF-8 JSON |
| 10 | Local Status | Status byte |
| 11 | Anchor Mode | Mode byte |
| 12 | File Progress | UTF-8 JSON |
| 13 | Message Status | Status + msg_id |
| 20 | Group Joined | GroupId + name |
| 21 | Group Message | Group + sender + content |
| 22 | Group Removed | UTF-8 GroupId |
| 23 | Group Roster / Sync Ref | UTF-8 GroupId or PeerId (triggers chat UI reload) |
| 24 | Group Invite | Inviter + group info |
| 25 | Profile Updated | Profile data |
| 26 | Join Request Recv | Request data |
| 27 | Join Request Rej | Rejection data |
| 30 | Swarm Stats | UTF-8 JSON |
| 31 | Direct Request | Peer info |
| 32 | Direct Accepted | Peer info |
| 33 | Handle Resolved | Handle + PeerId |
| 34 | Handle Verified | Handle + PeerId + sigs |
| 35 | Reaction/Fail | Reaction or failure |
| 36 | Retention Changed | UTF-8 PeerId |
| 37 | Message Deleted | UTF-8 msg_id |
| 38 | Message Edited | msg_id + content |
| 99 | Rust Debug Log | UTF-8 text (native logger diagnostic stream) |

## Memory Management

### Rules
1. All strings returned by Rust must be freed with `introvert_free_string`
2. All binary data returned by Rust must be freed with `introvert_free_binary`
3. Use `try/finally` blocks in Dart to ensure cleanup
4. Never access freed memory

### Example (Dart)
```dart
final result = _getClientMessage(peerId);
try {
  if (result.code == 0 && result.data != nullptr) {
    final message = result.data.cast<Utf8>().toDartString();
    // Process message
  }
} finally {
  _freeBinary(result.data, result.len);
}
```
