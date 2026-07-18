// ============================================================
// EXTRACTED FROM: src/intro_claw.rs (3406 lines total)
// Date: 2026-07-18
// Purpose: Expert consultation for same-network file transfer delay
// ============================================================

// ------------------------------------------------------------
// SECTION 1: Connection Optimizer — Direct Upgrade Logic (lines 618-630)
// ------------------------------------------------------------
/*
pub fn should_attempt_direct_upgrade(
    &self,
    peer_id: &str,
    is_currently_relayed: bool,
    has_mdns: bool,
    battery_ok: bool,
) -> bool {
    if !is_currently_relayed { return false; }
    if !battery_ok { return false; }
    // mDNS = same LAN, instant direct. Remote peers attempt DCUtR hole-punch.
    has_mdns || self.should_attempt_dcutr(peer_id)
}
*/

// KEY OBSERVATION: The connection optimizer CAN detect same-network peers
// via mDNS and suggest a direct upgrade. However, this is only used for
// "upgrade suggestions" in the IntroClaw tick — it does NOT force
// forward_to_mesh() to use direct P2P instead of relay.


// ------------------------------------------------------------
// SECTION 2: Recommended Path (lines 1788-1817)
// Determines whether to use direct or relay for a peer
// ------------------------------------------------------------
/*
pub fn get_recommended_path(&self, peer_id: &str, is_connected: bool, is_relayed: bool, has_mdns: bool) -> bool {
    // If connected directly and healthy, use direct P2P
    if is_connected && !is_relayed {
        let health = self.health_scorer.get_score(peer_id);
        if health < 0.3 {
            return true; // Use relay
        }
        return false; // Use direct
    }

    // If on same local network (mDNS), prefer direct
    if has_mdns && is_connected {
        return false; // Use direct
    }

    // If peer has been unstable, pre-establish relay
    if self.reconnection_scorer.should_pre_establish(peer_id) {
        return true; // Use relay
    }

    // Default: if not connected directly, use relay
    !is_connected || is_relayed
}
*/

// KEY OBSERVATION: get_recommended_path correctly identifies same-network
// peers (has_mdns && is_connected → direct). However, this function is
// only called from IntroClaw tick context — it is NOT consulted by
// forward_to_mesh() or dial_relay_path() when making routing decisions.
// The routing decision in forward_to_mesh is purely based on:
// 1. Is WebRTC open? → Use it
// 2. Is it a file payload? → Use gossipsub (which goes through relay)
// 3. Is peer connected? → Use direct libp2p
// 4. Otherwise → Dial relay path
// There is no check for "is this peer on the same LAN?"


// ------------------------------------------------------------
// SECTION 3: IntroClaw Tick — Direct Upgrade Candidates (lines 2230-2256)
// ------------------------------------------------------------
/*
for peer_id_str in &ctx.connected_peers {
    let peer_id = peer_id_str.parse::<PeerId>()?;
    let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
    let has_mdns = ctx.mdns_discovered.contains(peer_id_str);

    let is_target_peer = self.active_chat_peer.as_ref().map(|p| p == peer_id_str).unwrap_or(false);
    let should_upgrade = if is_target_peer && is_relayed {
        battery_ok
    } else {
        self.conn_optimizer.should_attempt_direct_upgrade(peer_id_str, is_relayed, has_mdns, battery_ok)
    };

    if should_upgrade {
        info!("[IntroClaw] Direct P2P upgrade candidate: {} (mDNS={}, battery={})", peer_id_str, has_mdns, battery_ok);
        upgrades.push(peer_id_str.clone());
    }
}
*/

// KEY OBSERVATION: IntroClaw correctly identifies mDNS peers as direct
// upgrade candidates. But the "upgrade" mechanism is not clear from this
// code — it collects candidates but the actual upgrade action (if any)
// happens elsewhere. The file transfer path in forward_to_mesh() does
// not consult this list.


// ------------------------------------------------------------
// SECTION 4: mDNS Peer Tracking
// ------------------------------------------------------------
/*
// In NetworkService (service.rs):
pub(crate) mdns_peers: HashSet<PeerId>,

// In network/mod.rs mDNS handler:
self.mdns_peers.insert(peer_id);

// In IntroClaw context:
pub mdns_discovered: Vec<String>,

// In ClawTickContext construction:
mdns_discovered: mdns_peers.iter().map(|p| p.to_string()).collect(),
*/

// The mdns_peers set is populated by the mDNS event handler and passed
// to IntroClaw as context. It is used for:
// 1. Connection optimizer (should_attempt_direct_upgrade)
// 2. Recommended path (get_recommended_path)
// 3. Logging ("Group relay: X peers, Y local (mDNS)")
//
// It is NOT used by:
// - forward_to_mesh() — the main routing function
// - dial_relay_path() — the dial strategy function
// - OutboundCircuitEstablished handler
// - InboundCircuitEstablished handler
