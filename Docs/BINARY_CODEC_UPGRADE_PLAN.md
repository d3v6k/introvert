# Binary Codec Upgrade Plan
## `/introvert/signaling/1.0.0` → `/introvert/signaling/2.0.0`

**Document Status:** Living reference — update as tasks complete  
**Created:** 2026-06-25  
**Author:** Engineering session (Antigravity + dev)  
**Related files:**
- [`src/network/codec.rs`](../src/network/codec.rs) — Binary codec implementation (v2.0.0, currently inactive)
- [`src/network/behaviour.rs`](../src/network/behaviour.rs) — Active codec config (currently v1.0.0)
- [`for_linux/src/network/behaviour.rs`](../for_linux/src/network/behaviour.rs) — RBN codec config (v1.0.0)
- [`src/network/types.rs`](../src/network/types.rs) — `SignalingRequest`/`SignalingResponse` canonical types

---

## 1. Why This Upgrade Exists

### The Problem with v1.0.0 (Current Protocol)

The current signaling protocol (`/introvert/signaling/1.0.0`) uses `libp2p::request_response::json::Behaviour`. Every payload — including raw file chunk data — is serialized as **JSON with base64-encoded binary**. This has a well-understood overhead:

- Base64 encoding adds **33% to every binary payload** (3 bytes → 4 chars)
- A 256 KB file chunk becomes ~341 KB on the wire
- A 512 KB chunk becomes ~682 KB on the wire
- At 70 Mbps throughput, this wastes ~17 Mbps of bandwidth on encoding alone
- On relay connections (100KB relay chunks), the overhead compounds with relay bandwidth costs

### The Solution: v2.0.0 Binary Codec

`src/network/codec.rs` implements a custom `libp2p::request_response::Codec` trait with a mixed binary/JSON wire format:

```
Wire format (per request):
  [0x01]          — version byte (1 byte)
  [flags: u8]     — bit 0 = has_binary_data (1 byte)
  [json_len: u32] — length of JSON section, big-endian (4 bytes)
  [json_payload]  — JSON bytes (variable)
  [data_len: u32] — binary data length (4 bytes, only if flag set)
  [raw_binary]    — raw file data (variable, only if flag set)
```

**For FileChunk payloads specifically:**
- The base64 string is stripped from the JSON
- Raw binary data is appended after the JSON section
- The receiver decodes it back to base64 (for in-memory compatibility) transparently

**Measured savings (from `codec_tests.rs`):**

| Chunk Size | v1.0.0 (JSON+B64) | v2.0.0 (Binary) | Savings |
|------------|-------------------|-----------------|---------|
| 64 KB | 87,433 bytes | 65,689 bytes | **~25%** |
| 256 KB | 349,609 bytes | 262,244 bytes | **~25%** |
| 512 KB | 699,209 bytes | 524,480 bytes | **~25%** |
| 1 MB | 1,398,409 bytes | 1,048,952 bytes | **~25%** |

At sustained 70 Mbps file transfer, this translates to a real-world gain of approximately **17–18 Mbps** of recovered bandwidth — or equivalently, the same transfer speed with 25% less network usage per device.

All non-FileChunk payloads (chat messages, signaling, ACKs, group actions) remain JSON — no change in size or compatibility risk for those paths.

---

## 2. Current State (as of 2026-06-25)

### What is DONE ✅

| Item | Status | Notes |
|------|--------|-------|
| Binary codec implementation | ✅ Complete | `src/network/codec.rs` — full `Codec` trait impl |
| Wire format specification | ✅ Complete | Version + flags + JSON + optional binary |
| Wire size tests | ✅ Complete | `codec_tests.rs` — validates 25% savings |
| Type naming conflict resolved | ✅ Complete | Renamed to `BinarySignalingRequest`/`BinarySignalingResponse` in codec.rs |
| Canonical types in `types.rs` | ✅ Complete | `SignalingRequest`/`SignalingResponse` added for v1.0.0 JSON codec |
| `cargo check` passes | ✅ Clean | Only warnings, no errors |
| macOS library built | ✅ Built | `make mac` succeeded 2026-06-25 |

### What is PARKED / DEACTIVATED ⏸️

| Item | Status | Notes |
|------|--------|-------|
| v2.0.0 codec ACTIVE in client | ⏸️ Parked | Reverted to v1.0.0 to restore cross-network connectivity |
| v2.0.0 codec in RBN firmware | ⏸️ Not started | RBN speaks v1.0.0 only |

### Why it was parked

The codec was introduced and activated in `behaviour.rs` during intro_claw integration work **before the RBN firmware was updated**. Since libp2p protocol negotiation requires **both sides to advertise the same protocol string**, the deployed Alibaba RBN (`47.89.252.80`) — which still speaks `/signaling/1.0.0` — rejected all connections from upgraded clients. This severed cross-network relay connectivity entirely.

**Fix applied 2026-06-25:** `behaviour.rs` reverted to v1.0.0 JSON codec. The `codec.rs` file is preserved in-tree, inert, waiting for the RBN upgrade.

---

## 3. What Needs to Be Done (Full Action Plan)

### Phase 1 — RBN Firmware: Add Dual-Protocol Support

> **Goal:** The Alibaba RBN must speak BOTH `/signaling/1.0.0` and `/signaling/2.0.0` simultaneously. This is the critical transition window — old clients keep working while new clients can take advantage of the binary codec.

**Why dual-protocol, not just swap?**
libp2p `request_response` negotiates the protocol during every new stream. If the RBN only speaks v2.0.0 and a user hasn't updated their app yet, their device cannot exchange any messages. The dual-protocol period must cover the full user upgrade window (estimated 2–4 weeks for a self-hosted app).

#### Step 1.1 — Copy `codec.rs` into `for_linux/src/network/`

The binary codec is currently only in `src/network/`. It needs to be mirrored to the RBN codebase.

**Files to copy/create:**
```
for_linux/src/network/codec.rs   ← copy from src/network/codec.rs
```

The RBN's `codec.rs` is identical to the client's — no changes needed. Both sides use the same wire format.

Also add to `for_linux/Cargo.toml`:
```toml
async-trait = "0.1"
futures = "0.3"
```
_(The client already has these; RBN Cargo.toml does not yet.)_

#### Step 1.2 — Update RBN `for_linux/src/network/mod.rs`

Add the module declaration:
```rust
// In for_linux/src/network/mod.rs
pub mod codec;
```

#### Step 1.3 — Update RBN `for_linux/src/network/types.rs`

Add `SignalingRequest` and `SignalingResponse` (they are already in the client's `types.rs`):
```rust
// At end of for_linux/src/network/types.rs
/// JSON-serialized request wrapper used by request_response::json::Behaviour (v1.0.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingRequest(pub SignalingPayload);

/// JSON-serialized response wrapper used by request_response::json::Behaviour (v1.0.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingResponse(pub String);
```

#### Step 1.4 — Update RBN `for_linux/src/network/behaviour.rs`: Dual-Protocol Behaviour

This is the core change. The `IntrovertBehaviour` struct currently has one `request_response` field. It needs a second field for the v2.0.0 binary protocol.

```rust
// for_linux/src/network/behaviour.rs

use crate::network::codec::{IntrovertCodec, BinarySignalingRequest, BinarySignalingResponse};
use crate::network::{SignalingRequest, SignalingResponse};

#[derive(NetworkBehaviour)]
pub struct IntrovertBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,

    // v1.0.0 — JSON codec (legacy, for devices not yet upgraded)
    pub request_response: request_response::json::Behaviour<SignalingRequest, SignalingResponse>,

    // v2.0.0 — Binary codec (new, for upgraded devices)
    pub request_response_v2: request_response::Behaviour<IntrovertCodec>,

    pub gossipsub: gossipsub::Behaviour,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: Toggle<relay::Behaviour>,
    pub autonat: autonat::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub connection_limits: connection_limits::Behaviour,
}

impl IntrovertBehaviour {
    pub fn new(...) -> Self {
        // ... existing v1.0.0 setup unchanged ...

        // Add v2.0.0 codec
        let binary_codec = IntrovertCodec::default();
        let request_response_v2 = request_response::Behaviour::with_codec(
            binary_codec,
            [(StreamProtocol::new("/introvert/signaling/2.0.0"), request_response::ProtocolSupport::Full)],
            rr_config.clone(),
        );

        Self {
            // ... existing fields ...
            request_response_v2,
        }
    }
}
```

#### Step 1.5 — Update RBN Event Handler in `for_linux/src/network/mod.rs`

The swarm event loop currently handles `IntrovertBehaviourEvent::RequestResponse(...)`. A second `RequestResponseV2(...)` variant will be generated by the `#[derive(NetworkBehaviour)]` macro. The handler must be added:

```rust
// In the main swarm event loop:
IntrovertBehaviourEvent::RequestResponseV2(event) => {
    match event {
        request_response::Event::Message { peer, message } => {
            match message {
                request_response::Message::Request {
                    request: BinarySignalingRequest(payload),
                    channel, ..
                } => {
                    // Respond immediately (fire-and-forget ack)
                    let _ = self.swarm.behaviour_mut().request_response_v2
                        .send_response(channel, BinarySignalingResponse("ok".to_string()));
                    // Route to the same handler — protocol-agnostic
                    self.handle_single_payload(peer, payload, true).await;
                }
                request_response::Message::Response {
                    request_id,
                    response: BinarySignalingResponse(_r)
                } => {
                    self.outbound_tracker_v2.remove(&request_id);
                }
            }
        }
        request_response::Event::OutboundFailure { peer, request_id, error } => {
            warn!("[Mesh] v2.0.0 outbound failure to {}: {:?}", peer, error);
            if let Some((peer_id, payload)) = self.outbound_tracker_v2.remove(&request_id) {
                // Fallback to v1.0.0
                let req_id = self.swarm.behaviour_mut().request_response
                    .send_request(&peer_id, SignalingRequest(payload.clone()));
                self.outbound_tracker.insert(req_id, (peer_id, payload));
            }
        }
        _ => {}
    }
}
```

> **Note:** The `handle_single_payload()` function is **protocol-agnostic** — it works on `SignalingPayload` regardless of how it arrived. No changes needed to that function or any message-handling logic.

#### Step 1.6 — Track Which Peers Support v2.0.0

In the `Identify` event handler in `for_linux/src/network/mod.rs`:

```rust
// When identify is received from a peer:
if info.protocols.iter().any(|p| p.as_ref() == "/introvert/signaling/2.0.0") {
    self.peer_supports_v2.insert(peer_id);
    info!("[Mesh] Peer {} supports v2.0.0 binary codec", peer_id);
} else {
    self.peer_supports_v2.remove(&peer_id);
}
```

Add to `NetworkService` struct (or wherever `NetworkService` is defined in `for_linux`):
```rust
pub(crate) peer_supports_v2: HashSet<PeerId>,
pub(crate) outbound_tracker_v2: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
```

#### Step 1.7 — Deploy Updated RBN Firmware

```bash
# From project root on dev machine:
./deploy_rbn.sh
```

The deploy script already handles everything:
1. Rsyncs `for_linux/src/` to `thinkpad.local` build machine
2. Cross-compiles with `cargo build --release --bin introvertd`
3. Copies binary back locally
4. Stops daemon on `47.89.252.80`
5. Uploads binary + service file
6. Restarts and verifies

---

### Phase 2 — Client: Re-activate v2.0.0 Codec

> **Goal:** Once the RBN firmware is deployed and confirmed speaking both protocols, upgrade the client to advertise and prefer v2.0.0.

**Prerequisite:** Phase 1 must be deployed and confirmed working on the Alibaba RBN before any client changes are made.

#### Step 2.1 — Update `src/network/behaviour.rs`: Add Second Request-Response Field

Same dual-protocol approach as the RBN:

```rust
// src/network/behaviour.rs
use crate::network::codec::{IntrovertCodec, BinarySignalingRequest, BinarySignalingResponse};

#[derive(NetworkBehaviour)]
pub struct IntrovertBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,

    // v1.0.0 — keep for backward compatibility during transition
    pub request_response: request_response::json::Behaviour<SignalingRequest, SignalingResponse>,

    // v2.0.0 — binary codec (active for file chunks)
    pub request_response_v2: request_response::Behaviour<IntrovertCodec>,

    // ... rest unchanged
}
```

In `IntrovertBehaviour::new()`:
```rust
let binary_codec = IntrovertCodec::default();
let request_response_v2 = request_response::Behaviour::with_codec(
    binary_codec,
    [(StreamProtocol::new("/introvert/signaling/2.0.0"), request_response::ProtocolSupport::Full)],
    rr_config.clone(),
);
```

#### Step 2.2 — Add `peer_supports_v2` and `outbound_tracker_v2` to `service.rs`

```rust
// src/network/service.rs
pub(crate) peer_supports_v2: HashSet<PeerId>,
pub(crate) outbound_tracker_v2: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
```

Initialise in `NetworkService::new()`:
```rust
peer_supports_v2: HashSet::new(),
outbound_tracker_v2: HashMap::new(),
```

#### Step 2.3 — Update `src/network/mod.rs`: Handle v2.0.0 Events

Add the `IntrovertBehaviourEvent::RequestResponseV2(...)` arm. Identical to the v1.0.0 arm but unwraps `BinarySignalingRequest` and uses `outbound_tracker_v2`.

Include automatic fallback: if v2.0.0 `OutboundFailure` fires, retry the payload over v1.0.0.

#### Step 2.4 — Selective v2.0.0 Sending for FileChunk

In `forward_to_mesh()`, change the direct libp2p send path:

```rust
// Choose codec based on payload type and peer capability
let req_id = if matches!(payload, SignalingPayload::FileChunk { .. })
    && self.peer_supports_v2.contains(&recipient_id)
{
    // Binary codec — 25% wire savings on file data
    info!("[Mesh] Sending FileChunk via v2.0.0 binary codec to {}", recipient_str);
    let id = self.swarm.behaviour_mut().request_response_v2
        .send_request(&recipient_id, BinarySignalingRequest(payload.clone()));
    self.outbound_tracker_v2.insert(id, (recipient_id, payload));
    id
} else {
    // JSON codec — all other payloads + fallback for peers not supporting v2.0.0
    let id = self.swarm.behaviour_mut().request_response
        .send_request(&recipient_id, SignalingRequest(payload.clone()));
    self.outbound_tracker.insert(id, (recipient_id, payload));
    id
};
```

#### Step 2.5 — Peer Protocol Discovery (Client Identify Handler)

```rust
// In the Identify event handler (src/network/mod.rs):
if info.protocols.iter().any(|p| p.as_ref() == "/introvert/signaling/2.0.0") {
    self.peer_supports_v2.insert(peer_id);
} else {
    self.peer_supports_v2.remove(&peer_id);
}
// Also clean up on ConnectionClosed:
self.peer_supports_v2.remove(&peer_id);
```

#### Step 2.6 — Build and Deploy Client

```bash
make mac      # macOS
make android  # Android
make ios      # iOS
```

---

### Phase 3 — Cleanup: Drop v1.0.0 Support (Future)

> **Trigger:** When >95% of active sessions use v2.0.0 (observable from RBN logs). Estimated 4–6 weeks after Phase 2 client release.

1. Remove `request_response` (v1.0.0) field from both `IntrovertBehaviour` structs
2. Remove `peer_supports_v2` guards — all sends go through v2.0.0
3. Remove `outbound_tracker` (v1.0.0 tracker) from service structs
4. Remove v1.0.0 event handlers from both `mod.rs` files
5. Deploy updated RBN firmware
6. Update this document: mark Phase 3 complete

---

## 4. Deployment Sequence

```
Phase 1:                          Phase 2:               Phase 3:
RBN Firmware                      Client Update          Cleanup

[1] Copy codec.rs to for_linux    [5] Add v2 field to   [10] Drop v1.0.0 from
[2] Add types to for_linux            behaviour.rs            behaviour (client)
    types.rs                      [6] Handle v2 events  [11] Drop v1.0.0 from
[3] Dual-protocol behaviour.rs    [7] Selective v2            RBN
[4] Deploy via ./deploy_rbn.sh        FileChunk sends   [12] Deploy final RBN
       │                          [8] Peer v2 tracking        firmware
       ▼                          [9] Build + deploy
   RBN speaks                            │
   BOTH 1.0.0                     Clients prefer v2
   AND 2.0.0                      for FileChunk only
       │                               │
       └── Confirm working ────────────┘
           before Phase 2             2–4 week window
                                      then Phase 3
```

**Rule:** Never move to the next phase until the previous phase is confirmed working on real devices across different networks.

---

## 5. Verification Checklist

### After Phase 1 (RBN dual-protocol deployed)

- [ ] RBN `identify` output lists BOTH `/introvert/signaling/1.0.0` AND `/introvert/signaling/2.0.0`
- [ ] Old client (v1.0.0 / v37) → still gets relay reservation from Alibaba RBN ✓
- [ ] Old client still delivers messages cross-network via relay ✓
- [ ] No increase in connection errors in RBN systemd logs (`journalctl -u introvertd -f`)

### After Phase 2 (Client updated)

- [ ] New client log shows `[Mesh] Sending FileChunk via v2.0.0 binary codec to ...`
- [ ] New client log shows `[Mesh] Peer <id> supports v2.0.0 binary codec` after identify
- [ ] File transfer between two new clients completes ~25% faster (or uses ~25% less data)
- [ ] File transfer between new client (v2.0.0) and old client (v1.0.0) completes correctly (fallback path)
- [ ] Chat, group messages, ACKs still arrive correctly (they use v1.0.0 path throughout transition)
- [ ] Two devices on different networks: relay reservation obtained, messages delivered ✓

---

## 6. Files Touched (Complete Map)

### Phase 1 — RBN Firmware Changes

| File | Change | Status |
|------|--------|--------|
| `for_linux/src/network/codec.rs` | **NEW** — copy of `src/network/codec.rs` | ✅ Complete |
| `for_linux/Cargo.toml` | Add `async-trait = "0.1"`, `futures = "0.3"` | ✅ Complete |
| `for_linux/src/network/mod.rs` | Add `pub mod codec;` declaration | ✅ Complete |
| `for_linux/src/network/types.rs` | Add `SignalingRequest`, `SignalingResponse` structs | ✅ Complete |
| `for_linux/src/network/behaviour.rs` | Add `request_response_v2` field + constructor code | ✅ Complete |
| `for_linux/src/network/mod.rs` | Add `RequestResponseV2` event handler, v2 outbound tracking, Identify v2 detection | ✅ Complete |
| Wherever `NetworkService` is defined in `for_linux` | Add `peer_supports_v2`, `outbound_tracker_v2` fields | ✅ Complete |
| `deploy_rbn.sh` | No changes needed — script is protocol-agnostic | ✅ Ready |

### Phase 2 — Client Changes

| File | Change | Status |
|------|--------|--------|
| `src/network/behaviour.rs` | Add `request_response_v2` field + constructor code | ✅ Complete |
| `src/network/service.rs` | Add `peer_supports_v2`, `outbound_tracker_v2` fields | ✅ Complete |
| `src/network/mod.rs` | Add `RequestResponseV2` event handler; selective v2 sending in `forward_to_mesh`; v2 Identify detection; ConnectionClosed cleanup | ✅ Complete |
| `src/network/codec.rs` | No changes needed — already complete | ✅ Ready |
| `src/network/types.rs` | No changes needed — types already added 2026-06-25 | ✅ Ready |

### Phase 3 — Cleanup (Future)

| File | Change | Status |
|------|--------|--------|
| `src/network/behaviour.rs` | Remove `request_response` (v1.0.0) field | 🔲 Future |
| `for_linux/src/network/behaviour.rs` | Remove `request_response` (v1.0.0) field | 🔲 Future |
| `src/network/mod.rs` | Remove v1.0.0 event handlers, `peer_supports_v2` guard | 🔲 Future |
| `for_linux/src/network/mod.rs` | Remove v1.0.0 event handlers | 🔲 Future |
| `src/network/service.rs` | Remove v1.0.0 `outbound_tracker` field | 🔲 Future |

---

## 7. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-06-25 (pre-fix) | Binary codec activated in `behaviour.rs` without RBN firmware update | Mistake — libp2p protocol negotiation requires both sides to advertise the same string. Broke all relay connectivity for cross-network users. |
| 2026-06-25 | Reverted `behaviour.rs` to v1.0.0 JSON codec. `codec.rs` kept in-tree with renamed types (`BinarySignalingRequest`/`BinarySignalingResponse`). | Restores cross-network connectivity immediately. Binary work preserved without loss. |
| 2026-06-25 | Chose dual-protocol transition (not hard cutover) | Hard cutover would break all devices not yet updated. Dual-protocol allows zero-downtime migration. |
| 2026-06-25 | v2.0.0 will send FileChunk payloads only (initially) | Largest savings are on file data. Lower risk than switching all payload types simultaneously. Chat/ACK/group actions stay on v1.0.0 through the transition. |
| — | Extend v2.0.0 to other binary payloads (GroupAction, MailboxDrain) | **Deferred** — evaluate after profiling Phase 2 impact in production. |

---

## 8. RBN Infrastructure Reference

| RBN | Address | Peer ID | Deploy |
|-----|---------|---------|--------|
| Alibaba Cloud (Primary) | `47.89.252.80:443` TCP + QUIC | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` | `./deploy_rbn.sh` |
| thinkpad.local (Secondary) | `192.168.1.81:8443` | `12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG` ⚠️ changes on restart | Manual SSH |

**Deploy Alibaba RBN (from project root):**
```bash
./deploy_rbn.sh
# Rsyncs source → thinkpad build machine → cross-compiles → deploys to 47.89.252.80
```

**Deploy thinkpad.local RBN:**
```bash
ssh dev@thinkpad.local
cd ~/introvert/for_linux
cargo build --release --bin introvertd
systemctl --user restart introvertd
journalctl --user -u introvertd -f  # verify
```

---

## 9. Quick Reference: Protocol Strings

| String | Codec | Behaviour Type | Status |
|--------|-------|----------------|--------|
| `/introvert/signaling/1.0.0` | JSON (`serde_json`) | `request_response::json::Behaviour<SignalingRequest, SignalingResponse>` | ✅ Active (client + RBN) |
| `/introvert/signaling/2.0.0` | Custom binary (`IntrovertCodec`) | `request_response::Behaviour<IntrovertCodec>` | ⏸️ Built, not deployed |
| `/introvert/kad/1.0.0` | libp2p Kademlia | Isolated DHT (not global IPFS DHT) | ✅ Active |
| `/introvert/id/1.0.0` | libp2p Identify | Peer metadata + protocol list exchange | ✅ Active |
| `/libp2p/circuit/relay/0.2.0/hop` | Circuit Relay v2 | Relay server (RBN/Anchor only) | ✅ Active |
| `/libp2p/circuit/relay/0.2.0/stop` | Circuit Relay v2 | Relay client (all nodes) | ✅ Active |

---

## 10. Lessons Learned — Do Not Repeat

> **NEVER activate a new libp2p protocol on the client without first deploying it to all RBNs.**
>
> libp2p protocol negotiation is binary — if the remote peer doesn't advertise the protocol, libp2p closes the stream immediately. For relay-dependent peers (all cross-network users), this means zero fallback when the RBN can't negotiate. The relay reservation itself fails, not just individual messages.

> **Always use dual-protocol during transitions.** libp2p `request_response` supports registering multiple protocol IDs by adding a second `Behaviour` field to the `#[derive(NetworkBehaviour)]` struct. The cost is minimal (one extra field). The benefit is zero-downtime migration.

> **The codec.rs binary savings are real and worth doing.** Measured 25% wire savings on all FileChunk payloads. At 70 Mbps transfer, that's ~17 Mbps recovered. The implementation is complete — it just needs the safe deployment path described in this document.
