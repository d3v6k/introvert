# RBN Phase 2 Implementation: Secure Registry, Anti-Fraud & Operator Rewards

**Version:** 1.0
**Date:** 2026-06-23
**Status:** Specification — Ready for Implementation
**Scope:** Solana on-chain RBN registry, anti-malicious-RBN defenses, reward claim hardening, operator wallet integration

---

## 1. Premise

The Introvert mesh network currently relies on a single hardcoded RBN (`47.89.252.80`) with **zero defenses** against a malicious operator. Any person can:

- Compile a modified `introvertd` that harvests all relayed metadata
- Register as an RBN with no stake at risk
- Silently drop, delay, or modify mailbox messages
- Perform traffic analysis on all relayed communications
- Submit fraudulent reward claims

The `is_lease_valid()` function always returns `true`. The `INTROVERT_TRUST_ALL_WITNESSES` flag bypasses RBN authorization. Reward proofs have no cryptographic signature. The treasury relay endpoint has no authentication.

**Phase 2 transforms RBNs from "trust me" to "verify me" through:**

1. On-chain Solana registry with mandatory 50,000 $INTR stake
2. Cryptographic message signatures preventing mailbox tampering
3. Envelope encryption so RBNs cannot read relayed content
4. Hardened reward claims with signed proofs and on-chain verification
5. Dynamic IP updates for home server operators
6. Multi-RBN redundancy to eliminate single points of failure

**A home server PC with a fast network connection CAN function as an effective RBN.** The daemon detects its public IP, registers on-chain, and updates automatically when the ISP changes the IP. Port 443 (TCP+UDP) must be forwarded.

---

## 2. Threat Model

### 2.1 Attack Vectors

| ID | Attack | Difficulty | Impact | Current Defense |
|----|--------|-----------|--------|-----------------|
| A1 | Run modified daemon, harvest metadata | Trivial | Critical | None |
| A2 | Drop/delay `MailboxStore` payloads | Trivial | Critical | None |
| A3 | Tamper 1-to-1 `ChatMessage` in mailbox | Easy | Critical | None |
| A4 | Submit fraudulent reward claims | Easy | High | None |
| A5 | Replay old reward proofs | Easy | High | 5-min cooldown only |
| A6 | Impersonate RBN via protocol advertisement | Easy | High | None |
| A7 | DDoS via Sybil RBNs (no stake required) | Easy | Medium | None |
| A8 | Forge group messages | Hard | High | **Mitigated** — Ed25519 signatures |
| A9 | Forge handle claims | Hard | Medium | **Mitigated** — PoW + witness quorum |
| A10 | Traffic analysis on relayed data | Inherent | Medium | None (architectural) |
| A11 | Treasury relay endpoint abuse | Easy | High | No authentication |
| A12 | Modify daemon to selectively censor users | Easy | High | None |

### 2.2 What a Malicious RBN Sees Today

The RBN has full visibility into:
- **All connecting PeerIds** — via Kademlia, Identify, relay reservations
- **All mailbox payloads in cleartext** — `ChatMessage` content, `FileTransfer` metadata (filename, hash, size, MIME), `Acknowledgement` states, group membership changes
- **Push tokens** — sent via `IdentifySleepState` payloads
- **Handle-to-PeerId mappings** — published to Kademlia DHT
- **Group manifests** — member lists, group names, descriptions

---

## 3. Current Security Gaps (Critical Bugs)

These are not design issues — they are broken code in production:

### 3.1 `is_lease_valid()` Always Returns True

**File:** `for_linux/src/economy/mod.rs:170-174`

```rust
pub fn is_lease_valid(&self, _balance: u64) -> bool {
    // RELAXED: Always return true for now to ensure connectivity during testing.
    true
}
```

The economic gate that should prevent zero-stake nodes from operating is completely disabled.

### 3.2 Lease Check Uses Wrong Public Key

**File:** `for_linux/src/network/mod.rs:793-795`

```rust
let local_pubkey = solana_client.get_treasury_pubkey();  // BUG: checks treasury, not operator
if let Ok(balance) = solana_client.fetch_balance(&local_pubkey).await {
```

The lease check validates the **treasury's** INTR balance, not the node operator's. Even if `is_lease_valid()` were implemented, it would check the wrong account.

### 3.3 `INTROVERT_TRUST_ALL_WITNESSES` Bypasses Authorization

**File:** `for_linux/src/network/mod.rs:4658`

This debug flag bypasses all RBN authorization for handle claims in production. It must be gated behind `#[cfg(debug_assertions)]` or removed.

### 3.4 Anchor Status Has No Cryptographic Proof

**File:** `for_linux/src/network/mod.rs:1199-1207`

A peer is classified as an anchor if it advertises both `/introvert/signaling/1.0.0` AND `/libp2p/circuit/relay/0.2.0/hop` via Identify. Any node can claim to be an anchor — there is no proof of stake or authorization.

---

## 4. On-Chain Solana Registry

### 4.1 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     SOLANA MAINNET                               │
│                                                                  │
│  ┌─────────────────────┐    ┌──────────────────────────────────┐│
│  │ Squads V4 Multisig  │───>│ introvert-registry (Anchor)      ││
│  │ (3-of-5 Admin)      │    │                                  ││
│  └─────────────────────┘    │  Instructions:                   ││
│                             │    register_rbn(peer_id, maddr)  ││
│                             │    update_multiaddr(new_addr)    ││
│                             │    heartbeat()                   ││
│                             │    unstake() -> 7-day cooldown   ││
│                             │    slash_rbn(node, reason)       ││
│                             │    claim_rewards(proof)          ││
│                             │                                  ││
│                             │  State: RbnNode {                ││
│                             │    operator_wallet: Pubkey       ││
│                             │    peer_id: [u8; 32]             ││
│                             │    multiaddr: String             ││
│                             │    stake_amount: u64             ││
│                             │    is_active: bool               ││
│                             │    last_heartbeat: i64           ││
│                             │    slashed: bool                 ││
│                             │    last_claim_nonce: u64         ││
│                             │  }                               ││
│  ┌─────────────────────┐    └──────────────────────────────────┘│
│  │ PDA Escrow Vault    │<── Program-Derived Address             │
│  │ (50k $INTR per RBN) │    No private key, immutable rules    │
│  └─────────────────────┘                                        │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Anchor Program Instructions

**New file:** `programs/introvert-registry/src/lib.rs`

#### `register_rbn(peer_id, multiaddr)`

1. Operator must hold >= 50,000 $INTR in their ATA
2. Transfer 50,000 $INTR from operator ATA to PDA escrow vault
3. Create `RbnNode` account with `is_active = true`
4. Record `multiaddr`, `peer_id`, `operator_wallet`
5. Set `last_heartbeat = now`

**Guards:**
- Operator must sign the transaction
- `peer_id` must be unique (no duplicate registrations)
- Operator cannot register more than one RBN per wallet

#### `update_multiaddr(new_multiaddr)`

1. Verify caller is the original `operator_wallet`
2. Verify node is not slashed and is active
3. Update `multiaddr` and `last_heartbeat`

**Purpose:** Dynamic IP updates for home servers. The daemon calls this whenever its public IP changes.

#### `heartbeat()`

1. Verify caller is the original `operator_wallet`
2. Verify node is not slashed
3. Update `last_heartbeat = now`

**Purpose:** Proves the RBN is alive. Required every 6 hours. Clients prune RBNs with stale heartbeats (>24h).

#### `unstake()`

1. Verify caller is the original `operator_wallet`
2. Set `is_active = false`
3. Start 7-day cooldown timer
4. After 7 days, operator can call `claim_unstake()` to receive stake back

**Purpose:** Orderly exit. Prevents sudden disappearance after attack.

#### `slash_rbn(node_account, reason)`

1. Only callable by the Squads V4 multisig (3-of-5)
2. Set `slashed = true`, `is_active = false`
3. Stake forfeited to treasury

**Slashable offenses:**
- Message tampering (client reports signed message that fails verification)
- Selective censorship (multiple clients report message drops)
- Reward fraud (duplicate or forged proofs detected on-chain)
- Sybil registration (same operator registering multiple RBNs)
- Uptime fraud (heartbeat sent but node unreachable for >24h)

#### `claim_rewards(proof)`

1. Verify proof signature against `provider_pubkey`
2. Verify `proof.nonce > last_claim_nonce` (replay protection)
3. Verify `proof.timestamp` is within 1 hour (freshness)
4. Calculate reward from `relayed_bytes`
5. Transfer INTR from reward escrow PDA to provider ATA
6. Update `last_claim_nonce`

### 4.3 Client Discovery Flow

**Modified file:** `src/network/config.rs`

Replace hardcoded bootstrap with Solana query:

```rust
pub async fn discover_rbn_nodes(rpc_url: &str, registry_id: &str) -> Vec<(PeerId, Multiaddr)> {
    let accounts = rpc_client.get_program_accounts(&registry_id).await?;

    let mut nodes = Vec::new();
    for (_, account) in accounts {
        let node = RbnNode::try_deserialize(&mut &account.data[..])?;

        // Must be: active, not slashed, sufficient stake, recent heartbeat
        if !node.is_active || node.slashed { continue; }
        if node.stake_amount < 50_000_000_000_000 { continue; }
        if now - node.last_heartbeat > 86400 { continue; }  // 24h staleness

        if let (Ok(addr), Ok(pid)) = (node.multiaddr.parse(), PeerId::from_bytes(&node.peer_id)) {
            nodes.push((pid, addr));
        }
    }

    // Fallback to hardcoded if no on-chain nodes found
    if nodes.is_empty() { return get_bootstrap_nodes(); }
    nodes
}
```

### 4.4 Dynamic IP Update for Home Servers

**Modified file:** `for_linux/src/network/mod.rs`

```rust
// In NetworkService::run() select loop:
_ = ip_check_interval.tick() => {
    if let Ok(current_ip) = detect_public_ip().await {
        let expected = format!("/ip4/{}/tcp/443", current_ip);
        if self.registered_multiaddr.as_ref() != Some(&expected) {
            info!("[RBN] IP changed: {} -> {}. Updating on-chain...",
                self.registered_multiaddr.as_deref().unwrap_or("unknown"), expected);

            if let Err(e) = self.update_onchain_multiaddr(&expected).await {
                error!("[RBN] On-chain update failed: {:?}", e);
            } else {
                self.registered_multiaddr = Some(expected);
                info!("[RBN] Multiaddr updated on-chain");
            }
        }
    }
}
```

**IP detection** (new helper):
```rust
async fn detect_public_ip() -> Result<String> {
    for service in &["https://api.ipify.org", "https://ifconfig.me/ip", "https://icanhazip.com"] {
        if let Ok(resp) = reqwest::get(*service).await {
            if let Ok(ip) = resp.text().await {
                let ip = ip.trim().to_string();
                if ip.parse::<std::net::IpAddr>().is_ok() { return Ok(ip); }
            }
        }
    }
    Err(anyhow::anyhow!("Could not detect public IP"))
}
```

**Check interval:** 5 minutes. Transaction cost: ~0.000005 SOL per update (< $0.01/month).

---

## 5. Operator Wallet Integration

### 5.1 Wallet Derivation

The operator's Solana wallet is derived from the master seed via HKDF-SHA256:

```
Master Seed (32 bytes)
    └── HKDF-SHA256(info="introvert_solana_wallet")
            └── ed25519 SigningKey
                    └── Solana Keypair
                            └── Public Address (base58)
```

**File:** `for_linux/src/identity.rs:59-68`

The wallet is **deterministically linked** to the node identity — same seed, same wallet. The wallet cannot be separated from the node.

### 5.2 RBN Operator Onboarding

```
Step 1: Obtain 50,000 $INTR
    └── Via DEX (Raydium/Orca), OTC, or ecosystem grant

Step 2: Transfer $INTR to derived wallet
    └── Wallet address shown via: introvertd --show-wallet
    └── Or in startup logs

Step 3: Start introvertd
    └── introvertd --seed-file /path/to/seed --relay --port 443

Step 4: Daemon automatically:
    a. Derives Solana wallet from seed
    b. Checks $INTR balance >= 50,000
    c. Detects public IP
    d. Calls register_rbn() on introvert-registry
    e. Transfers 50,000 $INTR to PDA escrow
    f. Starts heartbeat loop (every 6 hours)
    g. Starts IP change detection (every 5 minutes)

Step 5: Clients discover RBN via Solana query
```

### 5.3 Reward Payout Flow

RBN operators earn from two sources:

| Source | Pool Size (Year 1) | Metric | Claim Method |
|--------|-------------------|--------|-------------|
| RBN Infrastructure Pool | 8,219 INTR/day | relay_bytes + uptime_seconds | On-chain `claim_rewards()` |
| Daily Activity Rewards | 16,438 INTR/day (shared) | messaging + files + calls | Gasless treasury relay |

**Both pay to the operator's derived Solana wallet (ATA).**

The infrastructure pool is **uncapped** for RBN operators — relay bytes and uptime have no daily cap. The 1.5x availability multiplier kicks in at >= 22 hours uptime.

### 5.4 Operator Dashboard

Add `--status` flag to `introvertd`:

```
═══════════════════════════════════════════
 INTROVERT RBN STATUS
═══════════════════════════════════════════
 PeerId:        12D3KooW...
 Wallet:        9jauyK...
 Multiaddr:     /ip4/203.0.113.5/tcp/443
 Stake:         50,000 $INTR (locked)
 Status:        ACTIVE
 Uptime:        14d 6h 32m
 Connected:     847 peers
 Relayed:       12.4 GB today
 Pending:       23.7 $INTR
 Last Claim:    2026-06-22 14:30 UTC
 Last Heartbeat: 2026-06-23 08:00 UTC
 IP Changed:    2026-06-20 03:15 UTC
═══════════════════════════════════════════
```

---

## 6. Reward Claim Hardening

### 6.1 Current Vulnerabilities

| Vulnerability | File | Line | Impact |
|--------------|------|------|--------|
| Proof is unsigned JSON | `solana.rs` | 100-108 | Anyone can forge proofs |
| No replay nonce | `solana.rs` | 113-118 | Proofs replayable after cooldown |
| Treasury relay has no auth | `solana.rs` | 132-154 | Endpoint abuse |
| `is_lease_valid()` always true | `economy/mod.rs` | 170 | Zero-stake nodes claim rewards |
| `commit_reward_claim` before on-chain confirm | `lib.rs` | 497 | Phantom claims |
| `DAILY_REWARD_ESCROW` is placeholder | `daily_rewards.rs` | 11 | No on-chain escrow |

### 6.2 Signed Reward Proofs

**Current:** `RewardProof` is unsigned JSON in a Memo instruction.

**Phase 2:** Sign the proof with the operator's Solana keypair.

```rust
pub struct RewardProof {
    pub provider_pubkey: String,      // Solana wallet address
    pub consumer_peer_id: String,     // Peer being served
    pub relayed_bytes: u64,           // Bytes relayed
    pub timestamp: u64,               // Epoch seconds
    pub nonce: u64,                   // NEW: monotonic per provider
    pub signature: Vec<u8>,           // NEW: Ed25519 signature
}
```

**Signing:**
```rust
fn sign_proof(keypair: &Keypair, proof: &mut RewardProof) {
    let data = format!("{}{}{}{}{}",
        proof.provider_pubkey, proof.consumer_peer_id,
        proof.relayed_bytes, proof.timestamp, proof.nonce);
    proof.signature = keypair.sign_message(data.as_bytes()).as_ref().to_vec();
}
```

**Verification (on-chain):**
```rust
fn verify_proof(proof: &RewardProof) -> bool {
    let pubkey = Pubkey::from_str(&proof.provider_pubkey)?;
    let data = format!("{}{}{}{}{}",
        proof.provider_pubkey, proof.consumer_peer_id,
        proof.relayed_bytes, proof.timestamp, proof.nonce);
    let sig = Signature::from_slice(&proof.signature)?;
    sig.verify(&pubkey.to_bytes(), data.as_bytes()).is_ok()
}
```

### 6.3 On-Chain Claim Verification

The `claim_rewards` Anchor instruction:

1. Verifies the proof signature (provider actually signed it)
2. Checks nonce is higher than `last_claim_nonce` (replay protection)
3. Checks timestamp is within 1 hour (freshness)
4. Calculates reward from relayed bytes
5. Transfers INTR from reward escrow PDA to provider ATA
6. Updates `last_claim_nonce`

### 6.4 Treasury Relay Authentication

**Problem:** The treasury relay endpoint has no authentication.

**Solution:** HMAC-SHA256 authentication.

```rust
// Client side:
let payload = json!({ "transaction": base64_tx, "provider": pubkey, "timestamp": now, "nonce": n });
let payload_bytes = serde_json::to_vec(&payload)?;
let hmac = hmac_sha256(&shared_secret, &payload_bytes);

client.post(&treasury_url)
    .header("X-Introvert-Signature", hex::encode(hmac))
    .header("X-Introvert-Pubkey", &pubkey)
    .body(payload_bytes)
    .send().await?;
```

### 6.5 Claim Confirmation Before Commit

**Problem:** `commit_reward_claim()` is called after relay returns success, but the on-chain tx may have failed.

**Solution:** Wait for on-chain confirmation.

```rust
match solana.submit_reward_claim(&keypair, &proof).await {
    Ok(signature) => {
        match solana.confirm_transaction(&signature).await {
            Ok(true) => {
                tracker.commit_reward_claim(&consumer_id, amount);  // Only after confirmed
            }
            Ok(false) => callback_error(-6, "Transaction failed on-chain"),
            Err(e) => callback_error(-7, &format!("Confirmation error: {}", e)),
        }
    }
    Err(e) => callback_error(-4, &format!("Submit error: {}", e)),
}
```

---

## 7. Message-Level Security

### 7.1 ChatMessage Signatures

**Problem:** A malicious RBN can modify `ChatMessage` payloads in the mailbox. No sender signature exists.

**Solution:** Add Ed25519 signature to every `ChatMessage`.

**Modified struct** (`src/network/types.rs`):
```rust
SignalingPayload::ChatMessage {
    content: String,
    msg_id: String,
    timestamp: i64,
    reply_to: Option<String>,
    sender_signature: Option<Vec<u8>>,  // NEW
}
```

**Signing** (on send):
```rust
let sign_data = format!("{}{}{}", content, msg_id, timestamp);
let signature = identity.keypair.sign(sign_data.as_bytes());
```

**Verification** (on receive):
```rust
if let Some(ref sig_bytes) = sender_signature {
    let sig = ed25519_dalek::Signature::from_slice(sig_bytes)?;
    let sign_data = format!("{}{}{}", content, msg_id, timestamp);
    let contact = storage.get_contact(&peer_id)?;
    let pubkey = ed25519_dalek::VerifyingKey::from_bytes(&contact.p2p_pubkey)?;
    if pubkey.verify(sign_data.as_bytes(), &sig).is_err() {
        warn!("[Security] ChatMessage signature FAILED from {}", peer_id);
        return;  // Reject tampered message
    }
}
```

### 7.2 Mailbox Replay Protection

**Problem:** A malicious RBN can re-deliver old `MailboxDrained` messages.

**Solution:** Monotonic sequence number per peer pair.

```rust
// On send: storage.get_next_seq(&recipient_id) -> seq
// On receive: reject if seq <= storage.get_last_seen_seq(&sender_id)
```

**Storage change:** `contacts` table gets `last_seen_seq INTEGER DEFAULT 0` column.

### 7.3 Envelope Encryption for Mailbox

**Problem:** RBNs can read all `MailboxStore` payloads in cleartext.

**Solution:** Encrypt with sender+recipient X25519 shared secret.

```
Sender:
  shared = X25519(sender_static_secret, recipient_static_public)
  key = HKDF-SHA256(shared, "introvert_mailbox_envelope")
  envelope = AES-256-GCM(key, random_nonce, serialized_payload)
  -> MailboxStore { recipient_id, payload: envelope }

RBN:
  -> Stores opaque blob (cannot decrypt)

Recipient (MailboxDrained):
  shared = X25519(recipient_static_secret, sender_static_public)
  key = HKDF-SHA256(shared, "introvert_mailbox_envelope")
  payload = AES-256-GCM_decrypt(key, envelope)
```

The sender already has the recipient's `static_key` from contact exchange. No new key distribution needed.

---

## 8. Multi-RBN Redundancy

### 8.1 Problem

A single RBN is a single point of failure. If it goes offline or acts maliciously, all relayed communication stops.

### 8.2 Solution

- Clients connect to **all** discovered RBNs (from on-chain registry)
- `MailboxStore` is sent to **2+ anchors** for redundancy
- Messages are deduplicated on receive (via `msg_id`)
- If one RBN drops messages, another delivers them

### 8.3 Implementation

**Modified files:**
- `src/network/mod.rs` — Fan out `MailboxStore` to multiple anchors
- `src/network/mod.rs` — Deduplicate `MailboxDrained` messages by `msg_id`

```rust
// In forward_to_mesh, mailbox fallback:
let connected_anchors: Vec<PeerId> = self.discovered_anchors.iter()
    .filter(|pid| self.swarm.is_connected(pid))
    .cloned()
    .collect();

// Store on up to 2 anchors for redundancy
for anchor_id in connected_anchors.iter().take(2) {
    let req_id = self.swarm.behaviour_mut().request_response.send_request(
        anchor_id,
        SignalingRequest(SignalingPayload::MailboxStore {
            recipient_id: recipient_str.clone(),
            payload: bytes.clone(),
        })
    );
}
```

---

## 9. Implementation Phases

### Phase 2A: Fix Critical Security Gaps (2 hours)

| Task | File | Change |
|------|------|--------|
| Fix `is_lease_valid()` | `for_linux/src/economy/mod.rs:170` | Check operator balance >= 50,000 INTR |
| Fix lease check pubkey | `for_linux/src/network/mod.rs:793` | Use operator wallet, not treasury |
| Gate `INTROVERT_TRUST_ALL_WITNESSES` | `for_linux/src/network/mod.rs:4658` | `#[cfg(debug_assertions)]` |

### Phase 2B: Message Security (4 hours)

| Task | File | Change |
|------|------|--------|
| Add `sender_signature` to ChatMessage | `src/network/types.rs` | New optional field |
| Sign outgoing ChatMessage | `src/network/mod.rs` | Sign with Ed25519 in `forward_to_mesh` |
| Verify incoming ChatMessage | `src/network/mod.rs` | Verify in `handle_single_payload` |
| Mirror for RBN | `for_linux/src/network/mod.rs` | Same changes |
| Add sequence to mailbox payloads | `src/network/types.rs` | New `seq` field |
| Verify sequence on receive | `src/network/mod.rs` | Reject stale seq |

### Phase 2C: On-Chain Registry (2-3 days)

| Task | Type | Description |
|------|------|-------------|
| Create Anchor program | New | `programs/introvert-registry/` with all instructions |
| Deploy to devnet | New | Test deployment |
| Add Solana client methods | Modify `for_linux/src/economy/solana.rs` | `register_node()`, `update_multiaddr()`, `send_heartbeat()` |
| Replace hardcoded bootstrap | Modify `src/network/config.rs` | `discover_rbn_nodes()` async |
| Add IP detection loop | Modify `for_linux/src/network/mod.rs` | 5-min interval |
| CLI wallet/status display | Modify `for_linux/src/main.rs` | `--show-wallet`, `--status` |

### Phase 2D: Envelope Encryption (1 day)

| Task | File | Change |
|------|------|--------|
| Encrypt before MailboxStore | `src/network/mod.rs` | AES-256-GCM with X25519 shared secret |
| Decrypt after MailboxDrained | `src/network/mod.rs` | Derive same key, decrypt |
| RBN pass-through | `for_linux/src/network/mod.rs` | No change (stores opaque blobs) |

### Phase 2E: Reward Claim Hardening (1 day)

| Task | File | Change |
|------|------|--------|
| Add nonce to RewardProof | `src/economy/mod.rs` | New field, monotonic per provider |
| Sign reward proofs | `src/economy/solana.rs` | Ed25519 signature |
| Create claim Anchor instruction | `programs/introvert-registry/` | `claim_rewards` with full verification |
| Add on-chain confirmation | `src/economy/solana.rs` | Wait for tx before commit |
| Add HMAC to treasury relay | `src/economy/solana.rs` | Auth headers |

### Phase 2F: Multi-RBN Redundancy (2 days)

| Task | File | Change |
|------|------|--------|
| Connect to all discovered RBNs | `src/network/mod.rs` | Multi-anchor connections |
| Fan out MailboxStore | `src/network/mod.rs` | Store on 2+ anchors |
| Deduplicate delivery | `src/network/mod.rs` | By msg_id |
| Load balancing | `src/network/mod.rs` | Prefer lowest-latency RBN |

---

## 10. Verification Plan

### Phase 2A
```bash
cargo test --lib
cargo test --test economy_audit -- --nocapture
grep -n "INTROVERT_TRUST_ALL_WITNESSES" for_linux/src/network/mod.rs
# Should show #[cfg(debug_assertions)] guard
```

### Phase 2B
```bash
cargo test --test foundation_test --test group_file_transfer_audit
# Unit test: sign -> verify succeeds, tamper -> verify fails, wrong key -> fails
# Unit test: seq 1 accepted, seq 1 rejected, seq 2 accepted
```

### Phase 2C
```bash
anchor deploy --provider.cluster devnet
# Test register_rbn, update_multiaddr, heartbeat, unstake, slash_rbn
# Test client discovery: query registry, parse RBN list
```

### Phase 2D
```bash
cargo test --test mailbox_audit -- --nocapture
# Verify RBN sees opaque bytes, recipient can decrypt
```

### Phase 2E
```bash
cargo test --test economy_audit -- --nocapture
# Test: sign proof -> verify succeeds, tamper -> fails
# Test: nonce 1 accepted, nonce 1 rejected, nonce 2 accepted
```

### Phase 2F
```bash
# Deploy 2 RBNs, client connects to both
# Send to offline client, verify stored on both
# Kill one RBN, verify delivery from the other
```

---

## 11. Key Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| RBN Stake | 50,000 INTR | Economic gate |
| Edge Relay Threshold | 500 INTR | Active relay eligibility |
| Heartbeat Interval | 6 hours | Liveness proof |
| Heartbeat Expiry | 24 hours | Stale node pruning |
| Unbonding Period | 7 days | Exit scam prevention |
| Claim Cooldown | 300 seconds | Claim spam prevention |
| IP Check Interval | 300 seconds | Dynamic IP detection |
| Proof Expiry | 3600 seconds | Proof freshness |
| Slash Multisig | 3-of-5 | Governance control |
| Annual Decay | 0.8 | Emission reduction |
| RBN Daily Pool | 8,219 INTR/day | Year 1 infrastructure rewards |
| User Daily Pool | 16,438 INTR/day | Year 1 social rewards |
