// ============================================================
// EXTRACTED FROM: src/network/mod.rs (8193 lines total)
// Date: 2026-07-18
// Purpose: Expert consultation for same-network file transfer delay
// ============================================================

// ------------------------------------------------------------
// SECTION 1: mDNS Peer Discovery (lines 1649-1673)
// How local peers are discovered and tracked
// ------------------------------------------------------------
/*
IntrovertBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(list)) => {
    let mut grouped: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
    for (peer_id, addr) in list {
        grouped.entry(peer_id).or_default().push(addr);
    }
    for (peer_id, addrs) in grouped {
        info!("mDNS discovered peer: {} with {} addresses", peer_id, addrs.len());
        
        // Track mDNS peers for Intro-Claw context
        self.mdns_peers.insert(peer_id);
        
        // Check if this peer is a static bootstrap node
        let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
        if !is_bootstrap {
            info!("[Mesh] Clearing stale addresses for peer {} prior to applying new mDNS discoveries.", peer_id);
            self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
        }

        for addr in addrs {
            info!("  address: {}", addr);
            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
            // Dial the specific active address directly to bypass PeerId dial backoff
            let _ = self.swarm.dial(addr);
        }
    }
}
*/

// KEY OBSERVATION: mDNS discovers peers and dials them, but this only
// establishes a libp2p connection. It does NOT trigger direct file transfer
// setup or bypass the relay circuit path. The mdns_peers set is only used
// by IntroClaw for "upgrade candidate" suggestions.


// ------------------------------------------------------------
// SECTION 2: OutboundCircuitEstablished Handler (lines 1955-2022)
// When relay circuit is established for outbound traffic
// ------------------------------------------------------------
/*
libp2p::relay::client::Event::OutboundCircuitEstablished { relay_peer_id, limit } => {
    info!("[Relay] OutboundCircuitEstablished via {} (limit={:?})", relay_peer_id, limit);

    // Clear dial rate limiter for all peers with pending messages.
    let pending_peers: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
    for peer_id in &pending_peers {
        self.relay_dial_limiter.remove(peer_id);
    }

    // Dial peers with pending messages through the relay circuit NOW.
    for peer_id in &pending_peers {
        self.dial_relay_path(*peer_id, false);
    }

    // Flush pending messages after 1500ms delay
    if !pending_peers.is_empty() {
        let tx = self.command_tx.clone();
        let peers_with_payloads: Vec<(PeerId, Vec<SignalingPayload>)> = pending_peers.iter()
            .filter_map(|pid| self.pending_messages.remove(pid).map(|p| (*pid, p)))
            .collect();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            for (peer_id, payloads) in peers_with_payloads {
                for payload in payloads {
                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                }
            }
        });
    }

    // Flush pending DB chunks for all connected peers
    let storage = Arc::clone(&self.storage);
    let tx_db = self.command_tx.clone();
    let connected_peers: Vec<String> = self.swarm.connected_peers()
        .map(|p| p.to_string())
        .collect();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        for peer_str in connected_peers {
            if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 50) {
                // ... send each chunk via ForwardMeshSignaling
            }
        }
    });
}
*/


// ------------------------------------------------------------
// SECTION 3: InboundCircuitEstablished Handler (lines 2024-2104)
// When another peer connects through the relay to us
// ------------------------------------------------------------
/*
libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, limit } => {
    info!("[Relay] InboundCircuitEstablished from {} (limit={:?})", src_peer_id, limit);

    // Record which RBN this peer is behind (relay hint)
    if let Some(&rbn_id) = self.relay_reservations.iter().next() {
        self.relay_hints.insert(src_peer_id, rbn_id);
    }

    // Clear rate limiter for this peer
    self.relay_dial_limiter.remove(&src_peer_id);

    // DCUtR hole-punch attempt for potential direct upgrade
    let _ = self.swarm.dial(src_peer_id);

    // Single-flight flush with 30s timeout lock
    if self.flush_in_progress.contains_key(&src_peer_id) {
        // Skip — flush already running
    } else {
        self.flush_in_progress.insert(src_peer_id, Instant::now());

        // RAM flush: pending_messages for this peer
        // DB flush: dequeue_pending_chunks (up to 100 chunks)
        // Both run concurrently via join!, lock released once after both complete
        tokio::spawn(async move {
            // RAM flush: 150ms initial delay, 20ms between payloads
            // DB flush: 200ms initial delay, 50ms between chunks
            // ... sends via ForwardMeshSignaling
        });
    }
}
*/

// KEY OBSERVATION: On every InboundCircuitEstablished, the system calls
// dequeue_pending_chunks() which re-selects the same chunks from the DB
// because they are never deleted until FileTransferComplete. This causes
// the same chunks to be sent 3-9 times as seen in the logs.


// ------------------------------------------------------------
// SECTION 4: dial_relay_path (lines 2916-3033)
// How the system decides which path to use for dialing
// ------------------------------------------------------------
/*
fn dial_relay_path(&mut self, recipient_id: PeerId, for_file_chunk: bool) {
    // Exponential backoff: base 5s, max 300s (5 minutes)
    // File chunks skip the rate limiter

    // Strategy 1: Direct P2P (fastest, no relay overhead)
    if self.swarm.dial(recipient_id).is_ok() {
        dial_success = true;
    }

    // Strategy 2: Via RBNs
    // For text messages: one RBN by latency, break early
    // For file chunks: ALL RBNs, no break (no mailbox fallback)
    // Sort by ping latency, prioritize relay_hint RBN
    for &(rbn_id, ref rbn_addr) in &rbn_list {
        let relay_addr = rbn_addr.clone()
            .with(Protocol::P2p(*rbn_id))
            .with(Protocol::P2pCircuit)
            .with(Protocol::P2p(recipient_id));
        match self.swarm.dial(relay_addr.clone()) {
            Ok(_) => { dial_success = true; }
            Err(e) => { /* log error */ }
        }
    }

    // Strategy 3: Via connected anchor nodes
    // Strategy N+1: WebSocket tunnel fallback
}
*/

// KEY OBSERVATION: Strategy 1 (Direct P2P) is tried first, but if the peer
// is not directly connected (is_connected=false), the dial is queued.
// Strategy 2 (RBN relay) is tried immediately after. The relay dial
// often completes first because it goes through the already-connected RBN,
// establishing the relay circuit before the direct dial completes.
// Once the relay circuit is established, direct P2P is never re-evaluated.


// ------------------------------------------------------------
// SECTION 5: forward_to_mesh (lines 3035-3284)
// The main routing decision function
// ------------------------------------------------------------
/*
async fn forward_to_mesh(&mut self, recipient_id: PeerId, payload: SignalingPayload, force_mailbox: bool) -> Result<()> {
    // 1. Try WebRTC Data Channel if open (skip for large FileChunk on direct connections)
    
    // 2. Route file payloads through gossipsub via per-transfer topic
    //    This is BEFORE the is_connected check because cross-network peers
    //    are not directly connected — is_connected returns false for them.
    let is_file_payload = matches!(payload, FileChunk { .. } | FileChunkRequest { .. });
    if is_file_payload {
        let topic_str = format!("file-transfer-{}", transfer_id);
        let topic = IdentTopic::new(&topic_str);
        let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&topic);
        match self.swarm.behaviour_mut().gossipsub.publish(topic, bytes) {
            Ok(_) => {
                // On success: remove from pending_file_chunks DB
                if is_chunk_data {
                    let _ = self.storage.remove_pending_chunk(transfer_id, chunk_index);
                }
                return Ok(());
            }
            Err(e) => {
                // On failure: release in-flight flag, fall through to direct delivery
            }
        }
    }

    // 3. Try direct libp2p delivery if connected
    if self.swarm.is_connected(&recipient_id) {
        // Check in-flight limits (relay: 16, direct: 8)
        // Try Noise encryption for non-file payloads
        // Try binary v2.0.0 codec for file chunks
        // Send via request_response protocol
        return Ok(());
    }

    // 4. Not connected — dial relay path
    self.dial_relay_path(recipient_id, false);

    // 5. Fallback: Persistent Mesh Storage (Mailbox)
    // FileChunk: enqueue to pending_file_chunks DB (never RAM)
    // FileChunkRequest: RAM-only with redundancy filter
    // Other payloads: RAM buffer in pending_messages
}
*/

// KEY OBSERVATION: The routing order is:
// 1. WebRTC → 2. Gossipsub → 3. Direct libp2p → 4. Relay dial → 5. Mailbox/DB
// For same-network peers, step 3 should work (mDNS dials them).
// But if the gossipsub publish in step 2 succeeds (which it does via the
// relay circuit), the function returns early at step 2 and never reaches
// step 3. The gossipsub path goes through the relay, not direct P2P.


// ------------------------------------------------------------
// SECTION 6: Pending Messages & Chunk Queue Flow
// ------------------------------------------------------------

// When forward_to_mesh can't deliver immediately:
// - FileChunk → enqueue_pending_chunk() in SQLite (persistent)
// - FileChunkRequest → pending_messages in RAM
// - Other payloads → pending_messages in RAM

// When circuit re-establishes:
// - InboundCircuitEstablished → flush both RAM and DB
// - dequeue_pending_chunks() selects up to 100 chunks WHERE in_flight_since=0 OR stale
// - Marks them as in_flight but does NOT delete them
// - On gossipsub publish success → remove_pending_chunk() deletes from DB
// - On gossipsub publish failure → release_in_flight_chunk() resets flag

// THE BUG: If the circuit drops before all chunks are published, the
// in_flight_since timeout (30s) resets the flag, and the next circuit
// re-establishment re-selects the same chunks. This creates the
// "same chunks sent 3-9 times" pattern seen in the logs.
