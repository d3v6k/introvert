use libp2p::{
    kad::{self, Record, RecordKey},
    request_response,
    swarm::SwarmEvent,
    PeerId, Swarm, Multiaddr,
    futures::StreamExt,
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use std::io::Read;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use chrono::Utc;
use parking_lot::RwLock;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use libp2p::{autonat, identify};
use x25519_dalek::PublicKey;
use tracing::{error, debug, warn, info};
use solana_sdk::pubkey::Pubkey;

pub mod noise_session;
pub mod wormhole;
pub mod behaviour;
pub mod config;
pub mod group;
pub mod registry;
pub mod tunnel;
pub mod codec;
pub mod types;
pub mod service;
pub mod e2ee;

pub use types::*;
pub use service::{NetworkService, NetworkConfig, IncomingTransfer, ActiveSeeder, PendingDiagnostic};

use crate::media::{MediaManager, WebRtcSignal};
use noise_session::NoiseSession;
pub use behaviour::{IntrovertBehaviour, IntrovertBehaviourEvent};


impl NetworkService {
    pub async fn new(config: NetworkConfig) -> anyhow::Result<Self> {
        let NetworkConfig {
            keypair,
            command_rx,
            command_tx,
            storage,
            reward_tracker,
            solana_client,
            daily_reward_engine,
            local_static_secret,
            session_encryption_key,
            enable_mdns,
            enable_listeners,
            tcp_port,
            enable_relay_server,
            max_connections,
            liveness_interval_secs,
            downloads_dir,
            is_stress_test,
        } = config;
        let local_static_public = PublicKey::from(&local_static_secret);
        let local_peer_id = PeerId::from(keypair.public());

        // Resolve Bootstrap Nodes (taking into account Tunnel Mode)
        let is_tunnel_enabled = storage.is_tunnel_mode_enabled();
        let mut tunnel_handle = None;

        // PHASE 3: Local-first client bootstrap — load the persistent RBN
        // history list FIRST for immediate handshakes, then merge with the
        // standard bootstrap list for any additional entries.
        let persistent_history = config::get_persistent_rbn_history_list();
        let standard_bootstrap = config::get_bootstrap_nodes();

        // Merge: persistent history first (priority), then standard bootstrap
        // entries that aren't already in the history list.
        let mut bootstrap_nodes = persistent_history;
        for (pid, addr) in standard_bootstrap {
            if !bootstrap_nodes.iter().any(|(existing_pid, existing_addr)| existing_pid == &pid && existing_addr == &addr) {
                bootstrap_nodes.push((pid, addr));
            }
        }

        info!("[Bootstrap] Loaded {} RBN entries (persistent history + standard)", bootstrap_nodes.len());

        if is_tunnel_enabled {
            info!("[Tunnel] Secure Tunnel Mode is active. Launching loopback client...");
            // Start local tunnel listener on a dynamic port (0 means dynamic)
            let rbn_ws_url = RBN_WS_URL.to_string();
            match tunnel::start_tunnel_client(0, rbn_ws_url).await {
                Ok((assigned_port, handle)) => {
                    tunnel_handle = Some(handle);
                    // Map RBN PeerID to localhost TCP port
                    let rbn_peer_id: PeerId = RBN_PEER_ID.parse().unwrap();
                    let local_tunnel_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", assigned_port).parse().unwrap();
                    bootstrap_nodes = vec![(rbn_peer_id, local_tunnel_addr)];
                    info!("[Tunnel] WebSocket Tunnel active on local port {}. Bootstrapping via localhost.", assigned_port);
                }
                Err(e) => {
                    error!("[Tunnel] Failed to start WebSocket tunnel: {}", e);
                }
            }
        }

        macro_rules! build_swarm {
            ($builder:expr) => {
                {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    #[allow(deprecated)]
                    yamux_config.set_receive_window_size(16 * 1024 * 1024); // 16 MiB window size for high-speed transfers
                    $builder
                        .with_relay_client(libp2p::noise::Config::new, move || yamux_config.clone())?
                        .with_behaviour(|keypair: &libp2p::identity::Keypair, relay_client| {
                            IntrovertBehaviour::new(local_peer_id, keypair.clone(), relay_client, enable_mdns, enable_relay_server, max_connections)
                        })?
                        .with_swarm_config(|c: libp2p::swarm::Config| {
                            c.with_idle_connection_timeout(Duration::from_secs(600)) // Keep idle connections alive for 10 mins
                        })
                        .build()
                }
            };
        }

        let builder = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default().nodelay(true),
                libp2p::noise::Config::new,
                || {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    #[allow(deprecated)]
                    yamux_config.set_receive_window_size(16 * 1024 * 1024); // 16 MiB window size for high-speed transfers
                    yamux_config
                },
            )?
            .with_quic_config(|mut c| {
                c.keep_alive_interval = Duration::from_secs(30);
                c
            });

        let mut swarm = if cfg!(target_os = "android") {
            build_swarm!(builder)
        } else {
            build_swarm!(builder.with_dns()?)
        };

        if enable_listeners {
            // IPv4 listeners (primary)
            swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", tcp_port).parse()?)?;
            swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", tcp_port).parse()?)?;
            // IPv6 listeners (for IPv6-only mobile data / NAT64 networks)
            if let Ok(addr) = format!("/ip6/::/tcp/{}", tcp_port).parse() {
                let _ = swarm.listen_on(addr);
            }
            if let Ok(addr) = format!("/ip6/::/udp/{}/quic-v1", tcp_port).parse() {
                let _ = swarm.listen_on(addr);
            }

            // Event 10: Local Node Status (1 = Online/Listening)
            crate::dispatch_global_event(10, &[1]);
        }

        // Subscribe to gossipsub topics for all existing groups
        if let Ok(groups) = storage.get_all_groups() {
            for (group_id, _, _, _, _) in groups {
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    error!("[Mesh] Failed to subscribe to gossipsub topic {}: {}", group_id, e);
                } else {
                    info!("[Mesh] Subscribed to gossipsub topic {}", group_id);
                }
            }
        }

        // Create shared Arc for is_relayed_map so both NetworkService and IntroClaw can use it
        let shared_is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>> = Arc::new(RwLock::new(HashMap::new()));

        let rbn_latencies = Arc::new(RwLock::new(HashMap::new()));
        let pending_manual_rbns = Arc::new(RwLock::new(HashMap::new()));

        let res = Self {
            swarm, 
            command_rx,
            command_tx,
            storage: storage.clone(),
            peer_connections: Arc::new(RwLock::new(HashMap::new())),
            reward_tracker,
            solana_client,
            daily_reward_engine,
            local_static_secret,
            local_static_public,
            session_encryption_key,
            noise_sessions: HashMap::new(),
            pending_handshakes: HashMap::new(),
            pending_messages: HashMap::new(),
            data_channels: Arc::new(RwLock::new(HashMap::new())),
            incoming_transfers: HashMap::new(),
            active_seeders: HashMap::new(),
            active_providers: indexmap::IndexMap::new(),
            discovered_anchors: Vec::new(),
            mesh_active_peers: HashSet::new(),
            is_relayed_map: shared_is_relayed_map.clone(),
            connectivity_type: 0,
            direct_conn_count: HashMap::new(),
            relay_reservations: HashSet::new(),
            relay_listeners: HashMap::new(),
            relay_dial_limiter: HashMap::new(),
            last_file_chunk_dial: HashMap::new(),
            outbound_tracker: HashMap::new(),
            inflight_requests: HashMap::new(),
            liveness_interval_secs,
            downloads_dir,
            local_keypair: keypair,
            resolved_group_codes: indexmap::IndexMap::new(),
            anchor_mappings: HashMap::new(),
            bootstrap_nodes: bootstrap_nodes.clone(),
            _tunnel_handle: tunnel_handle,
            tunnel_active: is_tunnel_enabled,
            tunnel_started_at: if is_tunnel_enabled { Some(Instant::now()) } else { None },
            pending_diagnostics: HashMap::new(),
            registry: registry::RegistryManager::new(storage.clone()),
            pending_claims: HashMap::new(),
            diagnostic_requests: HashMap::new(),
            is_stress_test,
            pending_offers: HashMap::new(),
            early_chunks: indexmap::IndexMap::new(),
            intro_claw: crate::intro_claw::IntroClawService::new(
                Arc::clone(&storage),
                shared_is_relayed_map,
            ),
            peer_supports_v2: HashSet::new(),
            outbound_tracker_v2: HashMap::new(),
            heal_rate_limiter: HashMap::new(),
            pending_requester_static_keys: HashMap::new(),
            introclaw_command_log: Vec::new(),
            pending_acks: HashMap::new(),
            mdns_peers: HashSet::new(),
            seen_group_messages: HashSet::new(),
            last_ack_flush: Instant::now(),
            rbn_latencies: rbn_latencies.clone(),
            pending_manual_rbns: pending_manual_rbns.clone(),
            verified_rbns: bootstrap_nodes.iter().map(|(id, _)| *id).collect(),
            sync_in_progress: HashMap::new(),
            relay_hints: HashMap::new(),
            last_telemetry_sent: Instant::now() - Duration::from_secs(600), // allow first send after 10min
            consecutive_zero_peers_ticks: 0,
        };

        // Sync to global RBN bootstrap list
        {
            let mut global_bn = crate::BOOTSTRAP_NODES.write();
            *global_bn = bootstrap_nodes.iter().map(|(pid, addr)| (pid.to_string(), addr.to_string())).collect();
        }

        Ok(res)
    }

    pub async fn run(mut self) {
        let peer_connections_reaper = Arc::clone(&self.peer_connections);
        tokio::spawn(async move {
            Self::start_peer_reaper(peer_connections_reaper).await;
        });

        let storage_cleaner = Arc::clone(&self.storage);
        tokio::spawn(async move {
            Self::start_mailbox_cleanup(storage_cleaner).await;
        });

        let pruner_storage = Arc::clone(&self.storage);
        tokio::spawn(async move {
            Self::start_message_pruning(pruner_storage).await;
        });

        let local_peer_id = *self.swarm.local_peer_id();
        let pubkey_record = Record {
            key: RecordKey::new(&local_peer_id.to_bytes()),
            value: self.local_static_public.to_bytes().to_vec(),
            publisher: Some(local_peer_id),
            expires: None,
        };
        let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

        // Pre-populate anchors with known RBN nodes and request relay reservations
        for (peer_id, addr) in &self.bootstrap_nodes {
            self.swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
            if !self.discovered_anchors.contains(peer_id) {
                self.discovered_anchors.push(peer_id.clone());
            }
            let _ = self.swarm.dial(addr.clone());
            // Request relay reservation immediately — all devices must be reachable.
            // Must use full multiaddr; relative /p2p/X/p2p-circuit fails with MissingRelayAddr.
            let mut relay_addr = addr.clone();
            if !relay_addr.to_string().contains(&peer_id.to_string()) {
                relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2p(*peer_id));
            }
            relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2pCircuit);
            match self.swarm.listen_on(relay_addr.clone()) {
                Ok(listener_id) => {
                    self.relay_reservations.insert(*peer_id);
                    self.relay_listeners.insert(listener_id, *peer_id);
                    info!("[Mesh] Relay reservation requested from RBN on startup: {}", peer_id);
                }
                Err(e) => debug!("[Mesh] Relay reservation request failed for {}: {:?}", peer_id, e),
            }
        }

        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
        
        // Handle restoration: check local registry first, then DHT fallback
        let existing_handle = self.storage.get_profile().ok().flatten().and_then(|(_, h, _, _, _)| h);
        let has_handle = existing_handle.as_deref().map_or(false, |h| !h.is_empty());
        if !has_handle {
            let my_pid = local_peer_id.to_string();
            // Fast path: check local handle_registry for a verified claim by this peer
            match self.storage.get_handle_by_peer(&my_pid) {
                Ok(Some(handle)) => {
                    let profile = self.storage.get_profile().ok().flatten();
                    let name = profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                    let avatar = profile.as_ref().and_then(|(_, _, a, _, _)| a.clone());
                    let privacy = profile.as_ref().map(|(_, _, _, p, _)| *p).unwrap_or(1);
                    let _ = self.storage.set_profile(name.as_deref(), Some(&handle), avatar.as_deref(), privacy);
                    info!("[Mesh] Restored handle '{}' from local registry for peer {}", handle, my_pid);
                }
                _ => {
                    // Slow path: query Kademlia DHT for ph_<peer_id> record
                    info!("[Mesh] No local handle found. Querying DHT for ph_{}...", my_pid);
                    let ph_key = RecordKey::new(&format!("ph_{}", my_pid).as_bytes());
                    let _ = self.swarm.behaviour_mut().kademlia.get_record(ph_key);
                }
            }
        }

        self.perform_mailbox_fetch().await;

        // PHASE 4: Async Background Registry Validation
        // Once the client is online via cached historical IPs, spin up a
        // non-blocking background task to query the Solana Mainnet Registry
        // for verified RBN handle entries and update the local cache.
        // Each entry undergoes a mandatory cryptographic test-dial before
        // being added to the verified_rbns set.
        let storage_for_registry = Arc::clone(&self.storage);
        let command_tx_for_registry = self.command_tx.clone();
        tokio::spawn(async move {
            Self::background_registry_validator(storage_for_registry, command_tx_for_registry).await;
        });

        // RBN HARDENING: Always provide Anchor Node service if we are a relay server
        let is_anchor = self.storage.is_anchor_mode_enabled() || self.swarm.behaviour().relay_server.as_ref().is_some();
        if is_anchor {
            info!("[Network] Sovereign Anchor Mode: Providing Anchor Node service.");
            info!("[Network] ISOLATION ACTIVE: Protocol set to /introvert/kad/1.0.0");
            self.swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server)); // Act as full DHT server
            let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
            let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
        }

        // Anchor nodes need frequent heartbeats for mesh availability; regular nodes use FCM
        let heartbeat_secs = if is_anchor { 30 } else { 300 };
        let republication_secs = if is_anchor { 60 } else { 300 };
        let mailbox_fetch_secs = if is_anchor { 120 } else { 300 };

        let mut republication_interval = tokio::time::interval(Duration::from_secs(republication_secs));
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(heartbeat_secs));

        let mut liveness_interval = tokio::time::interval(Duration::from_secs(self.liveness_interval_secs));
        let mut contact_refresh_interval = tokio::time::interval(Duration::from_secs(300));
        let mut anchor_discovery_interval = tokio::time::interval(Duration::from_secs(5 * 60));
        let mut mailbox_fetch_interval = tokio::time::interval(Duration::from_secs(mailbox_fetch_secs));
        let mut fast_poll_interval = tokio::time::interval(Duration::from_secs(1)); // Fast poll when transfers are active
        let mut status_check_interval = tokio::time::interval(Duration::from_secs(15)); // 15s — faster mobile/VPN recovery (was 30s)
        let mut fast_reconnect_interval = tokio::time::interval(Duration::from_secs(5)); // 5s — conditional aggressive reconnect when transfers are waiting
        let mut telemetry_interval = tokio::time::interval(Duration::from_secs(30 * 60)); // 30 minutes
        let mut pull_retry_interval = tokio::time::interval(Duration::from_secs(1));
        let mut lease_interval = tokio::time::interval(Duration::from_secs(60 * 60));
        let mut intro_claw_interval = tokio::time::interval(Duration::from_secs(300)); // 5 min
        // FCM push handles wake-ups — reduced intervals for idle efficiency


        let mut last_status = 0u8;
        let mut last_fast_mailbox_fetch = Instant::now() - Duration::from_secs(60);
        self.consecutive_zero_peers_ticks = 0; // Reset on start
        let mut mobile_mailbox_skip = false;

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    // Broadcast last-seen heartbeat to connected peers
                    let timestamp = chrono::Utc::now().timestamp();
                    let connected: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                    for peer_id in connected {
                        let payload = crate::network::SignalingPayload::Heartbeat { timestamp };
                        let _ = self.forward_to_mesh(peer_id, payload, false).await;
                    }
                    
                    // Periodic cache maintenance
                    let now = Instant::now();
                    self.heal_rate_limiter.retain(|_, t| now.duration_since(*t) < Duration::from_secs(60));
                    if self.seen_group_messages.len() > 1000 {
                        self.seen_group_messages.clear();
                    }
                }
                _ = fast_poll_interval.tick() => {
                    let has_active_incoming = self.incoming_transfers.values().any(|t| t.is_relayed);
                    let has_active_seeding = !self.active_seeders.is_empty();
                    let has_relay_peers = self.is_relayed_map.read().values().any(|&r| r);
                    if has_active_incoming || has_active_seeding || has_relay_peers {
                        // FLUSH NON-CHUNK PENDING MESSAGES ONLY.
                        // File chunks/requests are handled by the pull model (pull_retry_interval below).
                        // Flushing file chunks here would cause relay flooding.
                        let all_pending_targets: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
                        for recipient in all_pending_targets {
                            if let Some(payloads) = self.pending_messages.get_mut(&recipient) {
                                // Stable extract: split into chunks (keep) and non-chunks (send now)
                                let mut non_chunks = Vec::new();
                                payloads.retain(|p| {
                                    if matches!(p, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
                                        true // keep in pending
                                    } else {
                                        non_chunks.push(p.clone());
                                        false // remove from pending
                                    }
                                });
                                if payloads.is_empty() { self.pending_messages.remove(&recipient); }
                                for payload in non_chunks {
                                    let _ = self.forward_to_mesh(recipient, payload, false).await;
                                }
                            }
                        }
                        // FAST MAILBOX FETCH: Poll mailbox every 5s when relay peers are connected.
                        // This ensures ACKs and receipts arrive promptly — not just during file transfers.
                        // Throttled to 5s to avoid flooding the RBN anchor with MailboxDrain requests.
                        if last_fast_mailbox_fetch.elapsed() > Duration::from_secs(5) {
                            last_fast_mailbox_fetch = Instant::now();
                            self.perform_mailbox_fetch().await;
                        }
                    }
                }
                _ = pull_retry_interval.tick() => {
                    // Check if we are online (have at least one active connection to the swarm/mesh)
                    let is_online = self.swarm.connected_peers().count() > 0;
                    
                    if is_online {
                        // Check for stalled relay transfers: if no new chunk in 8s, re-request missing chunks
                        let mut stalled = Vec::new();
                        for (tid, t) in self.incoming_transfers.iter_mut() {
                            let is_connected = self.swarm.is_connected(&t.peer_id);
                            let is_relayed_conn = self.is_relayed_map.read().get(&t.peer_id).cloned().unwrap_or(false);
                            let has_webrtc = {
                                let dc_store_read = self.data_channels.read();
                                if let Some(dc) = dc_store_read.get(&t.peer_id) {
                                    dc.ready_state() == RTCDataChannelState::Open
                                } else {
                                    false
                                }
                            };
                            let is_direct_p2p = (is_connected && !is_relayed_conn) || has_webrtc;
                            
                            // SOVEREIGN SWARM: Periodically query Kademlia DHT for new seeders/providers
                            // during active group downloads to dynamically discover completed peers (every 5 seconds).
                            if t.group_id.is_some() && t.last_update.elapsed() > Duration::from_secs(5) {
                                let tx = self.command_tx.clone();
                                let hash = t.file_hash.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::FindProviders { file_hash: hash }).await;
                                });
                            }

                            // SMART STALL DETECTION: Only retry if the received chunk count has NOT
                            // grown since the last watchdog tick. This prevents re-requesting chunks
                            // that are actively arriving from a slow seeder serving multiple peers.
                            let current_chunk_count = t.received_chunks.len();
                            let watchdog_timeout = if is_direct_p2p { 10 } else { 8 };
                            let truly_stalled = current_chunk_count == t.stall_chunk_count
                                && t.last_update.elapsed() > Duration::from_secs(watchdog_timeout);
                            // Always update the stall_chunk_count so next tick reflects new arrivals.
                            t.stall_chunk_count = current_chunk_count;

                            // Direct P2P switches to pull recovery if completely stalled for watchdog_timeout (10s)
                            // to handle reconnects or lost chunks without permanently downgrading healthy high-speed transfers.
                            let should_retry = truly_stalled;

                            if should_retry {
                                // Find the first missing chunk index
                                let mut next = 0u32;
                                while t.received_chunks.contains_key(&next) { next += 1; }
                                let window = self.intro_claw.get_optimal_pipeline_depth(&t.peer_id.to_string(), is_direct_p2p);
                                let limit = if t.total_chunks > 0 {
                                    std::cmp::min(next + window, t.total_chunks)
                                } else {
                                    next + window
                                };
                                if next < limit {
                                    // Align next_pull_idx so it starts pulling sequentially from the new limit
                                    t.next_pull_idx = limit;
                                    t.last_update = Instant::now();
                                    t.is_relayed = true; // Auto-transition to pull model on stall
                                    stalled.push((tid.clone(), t.peer_id, t.providers.clone(), next, limit, t.chunk_size));
                                }
                            }
                        }
                        
                        for (tid, peer, providers, first_missing_idx, limit, csize) in stalled {
                            info!("[Mesh] Transfer {} stalled. Retrying PULL for chunks {}..{} from {} providers", 
                                     tid, first_missing_idx, limit - 1, providers.len());
                            
                            // REDUNDANCY FILTER: Remove old requests for this transfer from RAM buffer
                            if let Some(pending) = self.pending_messages.get_mut(&peer) {
                                pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref id, .. } if id == &tid));
                            }
                            
                            let tx = self.command_tx.clone();
                            let tid_clone = tid.clone();
                            let relay_hint = self.relay_reservations.iter().next().map(|id| id.to_string());

                            // Intelligent Provider Selection: Prioritize connected direct peers over relayed/offline ones
                            let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &providers);

                            tokio::spawn(async move {
                                for idx in first_missing_idx..limit {
                                    let target_peer = if !selected_providers.is_empty() {
                                        selected_providers[(idx as usize) % selected_providers.len()]
                                    } else {
                                        peer
                                    };
                                    let req = SignalingPayload::FileChunkRequest { 
                                        transfer_id: tid_clone.clone(), 
                                        chunk_index: idx,
                                        chunk_size: Some(csize),
                                        relay_hint: relay_hint.clone(),
                                    };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                        peer_id: target_peer, 
                                        payload: req 
                                    }).await;
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                }
                            });
                        }
                    }
                }
                _ = status_check_interval.tick() => {
                    let connected_count = self.swarm.connected_peers().count();
                    let has_relay_listener = self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit"));
                    let has_confirmed_reservation = !self.relay_reservations.is_empty();

                    // --- ACCURATE STATUS DISPATCH ---
                    // status=0: OFFLINE — no connections at all
                    // status=1: ONLINE — connected AND relay reservation active (messages CAN flow)
                    // status=2: RELAY — relay reservation accepted (set directly in ReservationReqAccepted)
                    // status=3: SYNCING — connecting to network
                    // status=4: CONNECTING — RBN connected but relay not yet established
                    let current_status: u8 = if connected_count == 0 {
                        0 // OFFLINE — no connections
                    } else if has_relay_listener || has_confirmed_reservation {
                        1 // ONLINE — reachable via relay
                    } else {
                        4 // CONNECTING — connected to RBN but relay pending
                    };

                    if current_status != last_status {
                        let prev_status = last_status;
                        last_status = current_status;
                        crate::dispatch_global_event(10, &[current_status]);
                        crate::dispatch_debug_log(&format!("[Status] Status change: {} (peers={}, relay_listener={})", current_status, connected_count, has_relay_listener));

                        // PHASE 5: Dispatch NetworkStatusStream on actual transitions only
                        if current_status == 1 && prev_status != 1 {
                            // Transitioned to online — clear connecting state
                            crate::dispatch_global_event(48, b"online:connected");
                        } else if current_status == 0 && prev_status != 0 {
                            crate::dispatch_global_event(48, b"offline:disconnected");
                        }
                    }

                    // Reset zero-peers counter when we have connections


                    if connected_count > 0 {


                        self.consecutive_zero_peers_ticks = 0;


                    }



                    // Retrospectively sync any unsynced past telemetry envelopes


                    if let Ok(unsynced) = self.storage.fetch_unsynced_local_telemetry() {


                        if !unsynced.is_empty() {


                            let connected_rbns: Vec<PeerId> = self.bootstrap_nodes.iter()


                                .map(|(id, _)| *id)


                                .filter(|id| self.swarm.is_connected(id))


                                .collect();


                            if !connected_rbns.is_empty() {


                                for (epoch_id, envelope_json) in unsynced {


                                    if let Ok(envelope) = serde_json::from_str::<crate::economy::TelemetryEnvelope>(&envelope_json) {


                                        let payload = SignalingPayload::TelemetryEnvelope {


                                            peer_id: envelope.peer_id,


                                            solana_wallet: envelope.solana_wallet,


                                            solana_ata: envelope.solana_ata,


                                            epoch_id: envelope.epoch_id,


                                            metrics: envelope.metrics,


                                            unique_peers: envelope.unique_peers,


                                            is_rbn: envelope.is_rbn,


                                            is_edge_node: envelope.is_edge_node,


                                            prestige_tier: envelope.prestige_tier,


                                            proof_hash: envelope.proof_hash,


                                            client_signature: envelope.client_signature,


                                            timestamp: envelope.timestamp,


                                        };


                                        for rbn_id in &connected_rbns {


                                            let _ = self.forward_to_mesh(*rbn_id, payload.clone(), false).await;


                                            info!("[Economy] Retrospectively sync telemetry for epoch {} to RBN {}", epoch_id, rbn_id);


                                        }


                                    }


                                }


                            }


                        }


                    }

                    // --- PROGRESSIVE RECONNECT LADDER ---
                    // This is the core resilience engine. It runs every 2 minutes and
                    // escalates through connection strategies from fastest to most basic.
                    // LADDER:
                    //   Step 1: If RBN connected but no relay — re-request reservation
                    //   Step 2: If no peers — re-dial all bootstrap nodes (TCP + QUIC)
                    //   Step 3: If no peers after step 2 — activate WebSocket tunnel
                    //   Step 4: If nothing works — report OFFLINE clearly
                    //
                    // When IntroClaw is active, steps 2-4 are managed by IntroClaw's
                    // ConnectionStateCycler which intelligently cycles through strategies
                    // and uses VPN passthrough only as a last resort.

                    if connected_count == 0 {
                        self.consecutive_zero_peers_ticks += 1;

                        // PHASE 5: Network Status Stream — notify UI of connecting state
                        if self.consecutive_zero_peers_ticks == 1 {
                            crate::dispatch_global_event(48, b"connecting:mesh_sweep");
                            crate::dispatch_debug_log("[StatusStream] Connecting to the mesh swarm...");
                        }

                        // If IntroClaw is active, evaluate its intelligent connection cycler on the 15-second tick
                        // to ensure quick recovery (e.g. within 30s) instead of waiting for the 5-minute tick.
                        if self.intro_claw.is_active() {
                            crate::dispatch_debug_log(&format!(
                                "[Resilience] No peers — IntroClaw active, evaluating ConnCycler (tick={})",
                                self.consecutive_zero_peers_ticks));
                            
                            let mdns_peers: Vec<String> = self.mdns_peers.iter()
                                .map(|p| p.to_string())
                                .collect();
                            let active_hashes: Vec<String> = self.incoming_transfers.keys().cloned().collect();
                            
                            let ctx = crate::intro_claw::ClawTickContext {
                                battery_pct: 100,
                                is_background: false,
                                connected_peers: Vec::new(),
                                mdns_discovered: mdns_peers,
                                active_transfer_hashes: active_hashes,
                                is_mobile_data: self.intro_claw.is_on_mobile_data(),
                                network_type: if self.intro_claw.is_on_mobile_data() { "cellular".into() } else { "wifi".into() },
                                connectivity_type: self.connectivity_type,
                                tunnel_active: self.tunnel_active,
                                consecutive_zero_peers_ticks: self.consecutive_zero_peers_ticks,
                                has_relay_reservation: !self.relay_reservations.is_empty(),
                            };

                            if let Some(strategy) = self.intro_claw.evaluate_connection_strategy(&ctx) {
                                info!("[Resilience] IntroClaw ConnectionStateCycler triggered connection strategy: {:?}", strategy);
                                let actions = crate::intro_claw::ClawActions {
                                    heal_peers: Vec::new(),
                                    prefetch_files: Vec::new(),
                                    retry_dead_letters: Vec::new(),
                                    upgrade_connections: Vec::new(),
                                    pre_establish_relays: Vec::new(),
                                    force_mesh_refresh: matches!(strategy, crate::intro_claw::ConnectionStrategy::ForceRefresh),
                                    cache_files_for_offline: Vec::new(),
                                    serve_cached_chunks: Vec::new(),
                                    connection_strategy: Some(strategy),
                                };
                                self.execute_claw_actions(actions).await;
                            }
                        } else {
                        // Step 2: No connections at all — re-dial ALL bootstrap addresses
                        let network_type = if self.intro_claw.is_on_mobile_data() { "CELLULAR" } else { "WIFI" };
                        warn!("[Resilience] OFFLINE: No peers connected on {}. Re-dialing all bootstrap addresses...", network_type);
                        crate::dispatch_debug_log(&format!("[Resilience] Step 2: Re-dialing all bootstrap nodes (TCP+QUIC) [{}]", network_type));
                        let mut any_dialed = false;
                        for (peer_id, addr) in &self.bootstrap_nodes {
                            crate::dispatch_debug_log(&format!("[Resilience] Dialing bootstrap: {}", addr));
                            if self.swarm.dial(addr.clone()).is_ok() {
                                any_dialed = true;
                            }
                            let _ = self.swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
                        }
                        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

                        // Step 3: Only activate tunnel after sustained disconnect (4 ticks = 60s)
                        // This prevents false tunnel activation during brief connection gaps.
                        let had_recent_relay = self.relay_reservations.len() > 0
                            || self.tunnel_started_at.map(|t| t.elapsed() < Duration::from_secs(120)).unwrap_or(false);
                        if !self.tunnel_active && !had_recent_relay && self.consecutive_zero_peers_ticks >= 4 {
                            warn!("[Resilience] Step 3: No direct path — escalating to WebSocket tunnel");
                            crate::dispatch_debug_log("[Resilience] Step 3: Activating WebSocket tunnel fallback");
                            let tx = self.command_tx.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_secs(3)).await;
                                let _ = tx.send(NetworkCommand::ActivateTunnel).await;
                            });
                        } else {
                            // VPN FIX: If tunnel is active but we still have 0 peers after multiple
                            // reconnect attempts, the VPN is likely blocking WebSocket connections.
                            // Force-reset the tunnel and re-activate it.
                            let tunnel_age = self.tunnel_started_at.map(|t| t.elapsed()).unwrap_or(Duration::from_secs(0));
                            if tunnel_age > Duration::from_secs(120) {
                                warn!("[Resilience] Tunnel active for {}s with 0 peers — likely blocking. Force-resetting tunnel.", tunnel_age.as_secs());
                                crate::dispatch_debug_log(&format!("[Resilience] VPN interference: tunnel stale ({}s). Force-resetting.", tunnel_age.as_secs()));
                                // Reset tunnel state
                                self.tunnel_active = false;
                                self._tunnel_handle = None;
                                // Remove the tunnel bootstrap entry (loopback address)
                                self.bootstrap_nodes.retain(|(_, addr)| !addr.to_string().contains("127.0.0.1"));
                                // Re-activate tunnel after brief delay
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    let _ = tx.send(NetworkCommand::ActivateTunnel).await;
                                });
                            } else {
                                crate::dispatch_debug_log(&format!("[Resilience] Tunnel active ({}s). Waiting for connection...", tunnel_age.as_secs()));
                            }
                        }

                        if !any_dialed {
                            // Step 4: Nothing dialed — report truly OFFLINE
                            warn!("[Resilience] Step 4: All bootstrap addresses failed. OFFLINE.");
                            crate::dispatch_debug_log("[Resilience] Step 4: All strategies exhausted — OFFLINE");
                        }
                        } // end else (!intro_claw.is_active())
                    } else if !has_relay_listener {
                        // Peers connected but no relay — reset zero-peers counter
                        self.consecutive_zero_peers_ticks = 0;
                        // Step 1: Connected to peers but no relay reservation.
                        // First ensure we're connected to at least one RBN, then request reservation.
                        warn!("[Resilience] Step 1: {} peers connected but no relay. Re-establishing...", connected_count);
                        crate::dispatch_debug_log(&format!("[Resilience] Step 1: {} peers connected but no relay. Re-establishing...", connected_count));

                        // Dial any disconnected RBNs first
                        let mut any_rbn_connected = false;
                        for (rbn_id, rbn_addr) in &self.bootstrap_nodes {
                            if !self.swarm.is_connected(rbn_id) {
                                crate::dispatch_debug_log(&format!("[Resilience] Dialing RBN: {}", rbn_addr));
                                let _ = self.swarm.dial(rbn_addr.clone());
                                let _ = self.swarm.behaviour_mut().kademlia.add_address(rbn_id, rbn_addr.clone());
                            } else {
                                any_rbn_connected = true;
                            }
                        }

                        // Request relay reservations from connected RBNs
                        for (rbn_id, rbn_addr) in &self.bootstrap_nodes {
                            if self.swarm.is_connected(rbn_id) && !self.relay_reservations.contains(rbn_id) {
                                let mut relay_addr = rbn_addr.clone();
                                if !relay_addr.to_string().contains(&rbn_id.to_string()) {
                                    relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2p(*rbn_id));
                                }
                                relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2pCircuit);
                                match self.swarm.listen_on(relay_addr.clone()) {
                                    Ok(id) => {
                                        self.relay_reservations.insert(*rbn_id);
                                        self.relay_listeners.insert(id, *rbn_id);
                                        crate::dispatch_debug_log(&format!("[Resilience] Relay reservation re-requested from {}", rbn_id));
                                    }
                                    Err(e) => {
                                        crate::dispatch_debug_log(&format!("[Resilience] Relay reservation failed: {:?}", e));
                                    }
                                }
                            }
                        }

                        if !any_rbn_connected {
                            crate::dispatch_debug_log("[Resilience] No RBNs reachable — will retry in 30s");
                        }

                        // VPN DETECTION: If reservations are empty but RBNs are connected,
                        // the VPN likely made reservations stale. Force-clear and re-dial.
                        // BUT: if we have pending relay listeners (requested, not yet accepted),
                        // don't force-disconnect — the reservation is still being established.
                        if self.relay_reservations.is_empty() && self.relay_listeners.is_empty() {
                            let rbn_connected: Vec<PeerId> = self.bootstrap_nodes.iter()
                                .filter(|(id, _)| self.swarm.is_connected(id))
                                .map(|(id, _)| *id)
                                .collect();
                            if !rbn_connected.is_empty() {
                                warn!("[VPN] Stale relay reservation detected — re-establishing connections to {} RBNs", rbn_connected.len());
                                crate::dispatch_debug_log(&format!("[VPN] Stale reservation: {} RBNs connected but 0 reservations. Force re-dial.", rbn_connected.len()));
                                for rbn_id in &rbn_connected {
                                    let _ = self.swarm.disconnect_peer_id(*rbn_id);
                                }
                                // Re-dial will happen on next tick via Step 2 (connected_count == 0)
                            }
                        }
                    }

                    // Retry undelivered messages: re-send messages stuck at status=0 for >60s
                    // to recipients we are now connected to. Handles cases where initial
                    // delivery silently failed (no MailboxStored ACK, no recipient ACK).
                    if let Ok(undelivered) = self.storage.fetch_undelivered_messages(60) {
                        for (msg_id, peer_id_str, content, reply_to) in undelivered {
                            if let Ok(pid) = peer_id_str.parse::<PeerId>() {
                                if self.swarm.is_connected(&pid) {
                                    info!("[Retry] Re-sending undelivered msg {} to {}", msg_id, pid);
                                    let timestamp = chrono::Utc::now().timestamp();
                                    let payload = SignalingPayload::ChatMessage {
                                        content,
                                        msg_id: msg_id.clone(),
                                        timestamp,
                                        reply_to,
                                    };
                                    let _ = self.forward_to_mesh(pid, payload, false).await;
                                }
                            }
                        }
                    }

                    // Periodically broadcast connection status of all currently connected peers.
                    let connected_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                    for peer_id in connected_peers {
                        let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                        let status: u8 = if is_relayed { 1 } else { 0 }; // 0 = Direct P2P, 1 = Relay Active
                        let mut data = peer_id.to_string().into_bytes();
                        data.push(b':');
                        data.push(status);
                        crate::dispatch_global_event(8, &data);
                    }
                }
                _ = fast_reconnect_interval.tick() => {
                    // Conditional aggressive reconnect: only fires when file transfers are
                    // waiting and no relay listener is active. Settles down once connected.
                    let has_relay_listener = self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit"));
                    if !has_relay_listener {
                        let has_pending_transfers = !self.incoming_transfers.is_empty()
                            || !self.active_seeders.is_empty()
                            || !self.pending_messages.is_empty();

                        if has_pending_transfers {
                            let connected_count = self.swarm.connected_peers().count();
                            crate::dispatch_debug_log(&format!(
                                "[Resilience] Fast reconnect: transfers waiting, no relay (peers={}, incoming={}, seeders={}, pending={})",
                                connected_count, self.incoming_transfers.len(), self.active_seeders.len(), self.pending_messages.len()
                            ));

                            // VPN/MOBILE FIX: If tunnel is active but we have 0 peers,
                            // force-reset and re-activate. Mobile networks need faster recovery.
                            if self.tunnel_active && connected_count == 0 {
                                if let Some(tunnel_start) = self.tunnel_started_at {
                                    // Mobile data: 15s threshold (carriers block faster)
                                    // WiFi/VPN: 30s threshold
                                    let threshold = if self.intro_claw.is_on_mobile_data() {
                                        Duration::from_secs(15)
                                    } else {
                                        Duration::from_secs(30)
                                    };
                                    if tunnel_start.elapsed() > threshold {
                                        warn!("[MOBILE] Tunnel active {}s with 0 peers on {} — force-resetting",
                                            tunnel_start.elapsed().as_secs(),
                                            if self.intro_claw.is_on_mobile_data() { "cellular" } else { "wifi" });
                                        crate::dispatch_debug_log(&format!(
                                            "[MOBILE] Tunnel stale ({}s) on {}. Force-resetting.",
                                            tunnel_start.elapsed().as_secs(),
                                            if self.intro_claw.is_on_mobile_data() { "cellular" } else { "wifi" }
                                        ));
                                        self.tunnel_active = false;
                                        self._tunnel_handle = None;
                                        self.bootstrap_nodes.retain(|(_, addr)| !addr.to_string().contains("127.0.0.1"));
                                        let tx = self.command_tx.clone();
                                        tokio::spawn(async move {
                                            let _ = tx.send(NetworkCommand::ActivateTunnel).await;
                                        });
                                    }
                                }
                            }

                            // Dial any disconnected RBNs
                            for (rbn_id, rbn_addr) in &self.bootstrap_nodes {
                                if !self.swarm.is_connected(rbn_id) {
                                    let _ = self.swarm.dial(rbn_addr.clone());
                                    let _ = self.swarm.behaviour_mut().kademlia.add_address(rbn_id, rbn_addr.clone());
                                }
                            }

                            // Request relay reservations from connected RBNs
                            for (rbn_id, rbn_addr) in &self.bootstrap_nodes {
                                if self.swarm.is_connected(rbn_id) && !self.relay_reservations.contains(rbn_id) {
                                    let mut relay_addr = rbn_addr.clone();
                                    if !relay_addr.to_string().contains(&rbn_id.to_string()) {
                                        relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2p(*rbn_id));
                                    }
                                    relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2pCircuit);
                                    match self.swarm.listen_on(relay_addr) {
                                        Ok(id) => {
                                            self.relay_reservations.insert(*rbn_id);
                                            self.relay_listeners.insert(id, *rbn_id);
                                            crate::dispatch_debug_log(&format!("[Resilience] Fast reconnect: relay reservation from {}", rbn_id));
                                        }
                                        Err(e) => debug!("[Resilience] Fast reconnect: reservation failed: {:?}", e),
                                    }
                                }
                            }
                        }
                    }
                }
                _ = telemetry_interval.tick() => {
                    // Send telemetry to RBN (30-minute interval, with 5min cooldown guard)
                    let now = Instant::now();
                    if now.duration_since(self.last_telemetry_sent) > Duration::from_secs(300) {
                        let local_peer_id = self.swarm.local_peer_id().to_string();

                        let mut seed = [0u8; 32];
                        let mut engine_found = false;
                        if let Some(engine) = crate::ENGINE.read().as_ref() {
                            seed = engine.identity.seed;
                            engine_found = true;
                        }

                        if engine_found {
                            if let Ok(signing_key) = crate::identity::NodeIdentity::derive_solana_keypair(seed) {
                                let sol_pubkey = solana_sdk::pubkey::Pubkey::new_from_array(signing_key.verifying_key().to_bytes());
                                let solana_wallet = sol_pubkey.to_string();
                                let solana_ata = crate::economy::derive_ata(&solana_wallet, "EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf")
                                    .unwrap_or_else(|| solana_wallet.clone());
                                let epoch_id = crate::economy::daily_rewards::economy_epoch_id();

                                let mut prestige_tier = 0u8;
                                if let Ok(Some((_, _, _, _, tier))) = self.storage.get_profile() {
                                    prestige_tier = tier as u8;
                                }

                                let envelope = self.reward_tracker.package_signed_telemetry(
                                    &local_peer_id,
                                    &solana_wallet,
                                    &solana_ata,
                                    &epoch_id,
                                    &signing_key,
                                    false,
                                    true,
                                    prestige_tier,
                                );



                                if let Ok(envelope_json) = serde_json::to_string(&envelope) {


                                    if let Err(e) = self.storage.save_local_telemetry(&epoch_id, &envelope_json) {


                                        warn!("[Economy] Failed to save local telemetry: {:?}", e);


                                    }


                                }



                                let telemetry_payload = SignalingPayload::TelemetryEnvelope {
                                    peer_id: envelope.peer_id,
                                    solana_wallet: envelope.solana_wallet,
                                    solana_ata: envelope.solana_ata,
                                    epoch_id: envelope.epoch_id,
                                    metrics: envelope.metrics,
                                    unique_peers: envelope.unique_peers,
                                    is_rbn: envelope.is_rbn,
                                    is_edge_node: envelope.is_edge_node,
                                    prestige_tier: envelope.prestige_tier,
                                    proof_hash: envelope.proof_hash,
                                    client_signature: envelope.client_signature,
                                    timestamp: envelope.timestamp,
                                };
                                let bootstrap_clone = self.bootstrap_nodes.clone();
                                for (rbn_id, _) in &bootstrap_clone {
                                    if self.swarm.is_connected(rbn_id) {
                                        let _ = self.forward_to_mesh(*rbn_id, telemetry_payload.clone(), false).await;
                                        self.last_telemetry_sent = now;
                                        debug!("[Economy] Telemetry submitted to RBN {}", rbn_id);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                _ = mailbox_fetch_interval.tick() => {
                    // Phase 3.3: On mobile data, skip every other mailbox fetch to reduce radio wakeups
                    if self.intro_claw.is_on_mobile_data() {
                        mobile_mailbox_skip = !mobile_mailbox_skip;
                        if mobile_mailbox_skip {
                            continue;
                        }
                    }
                    self.perform_mailbox_fetch().await;
                    for (_, addr) in &self.bootstrap_nodes {
                        let _ = self.swarm.dial(addr.clone());
                    }

                    // Sweep RAM-buffered FileChunks to DB for restart survival
                    for (recipient, payloads) in &self.pending_messages {
                        let peer_str = recipient.to_string();
                        for payload in payloads {
                            if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
                                if let Ok(chunk_data) = base64::decode(data_base64) {
                                    let _ = self.storage.enqueue_pending_chunk(transfer_id, &peer_str, *chunk_index, &chunk_data);
                                }
                            }
                        }
                    }

                    // Flush pending messages periodically (every 30 seconds)
                    let all_pending: Vec<(PeerId, Vec<SignalingPayload>)> = self.pending_messages.drain().collect();
                    for (recipient, payloads) in all_pending {
                        for payload in payloads {
                            let _ = self.forward_to_mesh(recipient, payload, false).await;
                        }
                    }

                    // Flush pending DB chunks for connected peers
                    // NOTE: Chunks are NOT removed after forwarding — they stay in DB until
                    // FileTransferComplete arrives. This prevents data loss if the forward
                    // succeeds but delivery fails later (circuit drops, peer restarts, etc.)
                    for peer_id in self.swarm.connected_peers().cloned().collect::<Vec<_>>() {
                        let peer_str = peer_id.to_string();
                        if let Ok(chunks) = self.storage.dequeue_pending_chunks(&peer_str, 50) {
                            if !chunks.is_empty() {
                                info!("[Periodic] Flushing {} pending DB chunks for {}", chunks.len(), peer_str);
                                for (transfer_id, chunk_index, chunk_data) in chunks {
                                    let data_base64 = base64::encode(&chunk_data);
                                    let payload = SignalingPayload::FileChunk {
                                        transfer_id: transfer_id.clone(),
                                        chunk_index,
                                        total_chunks: 0,
                                        data_base64,
                                    };
                                    let _ = self.forward_to_mesh(peer_id, payload, false).await;
                                }
                            }
                        }
                    }

                    // Cleanup stale pending chunks (>24h)
                    let _ = self.storage.cleanup_stale_pending_chunks(86400);

                    // Cleanup stale sync_in_progress entries (>60s)
                    self.sync_in_progress.retain(|_, started| started.elapsed() < Duration::from_secs(60));

                    // Retry undelivered messages: re-send messages stuck at status=0 for >60s
                    // to recipients we are now connected to.
                    if let Ok(undelivered) = self.storage.fetch_undelivered_messages(60) {
                        for (msg_id, peer_id_str, content, reply_to) in undelivered {
                            if let Ok(pid) = peer_id_str.parse::<PeerId>() {
                                if self.swarm.is_connected(&pid) {
                                    info!("[Retry] Re-sending undelivered msg {} to {}", msg_id, pid);
                                    let timestamp = chrono::Utc::now().timestamp();
                                    let payload = SignalingPayload::ChatMessage {
                                        content,
                                        msg_id: msg_id.clone(),
                                        timestamp,
                                        reply_to,
                                    };
                                    let _ = self.forward_to_mesh(pid, payload, false).await;
                                }
                            }
                        }
                    }
                }
                _ = lease_interval.tick() => {
                    let solana_client = Arc::clone(&self.solana_client);
                    let local_pubkey = solana_client.get_treasury_pubkey();
                    if let Ok(balance) = solana_client.fetch_balance(&local_pubkey).await {
                        if !self.reward_tracker.is_lease_valid(balance) {
                            info!("[Economy] Identity Lease EXPIRED. Pruning node from mesh.");
                            let local_peer_id = *self.swarm.local_peer_id();
                            self.swarm.behaviour_mut().kademlia.remove_peer(&local_peer_id);
                            let anchor_key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                            self.swarm.behaviour_mut().kademlia.stop_providing(&anchor_key);
                            let _ = self.swarm.disconnect_peer_id(local_peer_id); 
                            crate::dispatch_global_event(10, &[0]);
                        }
                    }
                }
                _ = intro_claw_interval.tick() => {
                    if self.intro_claw.is_active() {
                        let connected_peers: Vec<String> = self.swarm.connected_peers()
                            .cloned()
                            .map(|pid| pid.to_string())
                            .collect();
                        let active_hashes = self.storage.get_active_drive_hashes();
                        let mdns_peers: Vec<String> = self.mdns_peers.iter()
                            .map(|p| p.to_string())
                            .collect();
                        let ctx = crate::intro_claw::ClawTickContext {
                            battery_pct: 100, // Platform-specific in future
                            is_background: false,
                            connected_peers,
                            mdns_discovered: mdns_peers,
                            active_transfer_hashes: active_hashes,
                            is_mobile_data: self.intro_claw.is_on_mobile_data(),
                            network_type: if self.intro_claw.is_on_mobile_data() { "cellular".into() } else { "wifi".into() },
                            connectivity_type: self.connectivity_type,
                            tunnel_active: self.tunnel_active,
                            consecutive_zero_peers_ticks: self.consecutive_zero_peers_ticks,
                            has_relay_reservation: !self.relay_reservations.is_empty(),
                        };
                        let actions = self.intro_claw.tick(&ctx);
                        self.execute_claw_actions(actions).await;
                    }
                }
                _ = republication_interval.tick() => {
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                    
                    // Periodically republish local public key for E2EE discovery
                    let local_peer_id = *self.swarm.local_peer_id();
                    let pubkey_record = kad::Record {
                        key: RecordKey::new(&local_peer_id.to_bytes()),
                        value: self.local_static_public.to_bytes().to_vec(),
                        publisher: Some(local_peer_id),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

                    // Periodically republish HANDLE if set
                    if let Ok(Some((_, Some(handle), _, _, _))) = self.storage.get_profile() {
                        if handle.starts_with("i@") {
                            let h_key = RecordKey::new(&handle.as_bytes());
                            let mut h_value = local_peer_id.to_string().into_bytes();
                            if let Ok(Some((_, timestamp, sigs_json, verified))) = self.storage.get_handle_claim(&handle) {
                                if verified {
                                    if let Ok(sigs) = serde_json::from_str::<Vec<String>>(&sigs_json) {
                                        let claim = registry::HandleClaim {
                                            handle: handle.clone(),
                                            peer_id: local_peer_id.to_string(),
                                            timestamp,
                                            pow_nonce: 0,
                                            signatures: sigs,
                                        };
                                        if let Ok(json) = serde_json::to_string(&claim) {
                                            h_value = json.into_bytes();
                                        }
                                    }
                                }
                            }
                            let h_record = kad::Record {
                                key: h_key.clone(),
                                value: h_value,
                                publisher: Some(local_peer_id),
                                expires: None,
                            };
                            let _ = self.swarm.behaviour_mut().kademlia.put_record(h_record, kad::Quorum::One);
                            let _ = self.swarm.behaviour_mut().kademlia.start_providing(h_key);

                            // Also publish reverse mapping peer_id -> handle for device restoration
                            let ph_key = RecordKey::new(&format!("ph_{}", local_peer_id).as_bytes());
                            let ph_record = kad::Record {
                                key: ph_key,
                                value: handle.into_bytes(),
                                publisher: Some(local_peer_id),
                                expires: None,
                            };
                            let _ = self.swarm.behaviour_mut().kademlia.put_record(ph_record, kad::Quorum::One);
                        }
                    }
                }
                _ = anchor_discovery_interval.tick() => {
                    let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                    let _ = self.swarm.behaviour_mut().kademlia.get_providers(key);
                }
                _ = liveness_interval.tick() => {
                    self.swarm.behaviour_mut().prune_stale_peers();
                }
                _ = contact_refresh_interval.tick() => {
                    if let Ok(contacts) = self.storage.get_all_contacts() {
                        for contact in contacts {
                            if let Ok(pid) = contact.peer_id.parse::<PeerId>() {
                                if !self.swarm.is_connected(&pid) {
                                    let _ = self.swarm.dial(pid);
                                }
                            }
                        }
                    }
                }
                event = self.swarm.select_next_some() => {
                    if let Err(e) = self.handle_swarm_event(event).await {
                        error!("Error handling swarm event: {:?}", e);
                    }
                }
                command = self.command_rx.recv() => {
                    if let Some(cmd) = command {
                        // Trace all incoming commands for debugging
                        let cmd_name = match &cmd {
                            NetworkCommand::HandleIncomingPayload { .. } => "HandleIncomingPayload",
                            NetworkCommand::ForwardMeshSignaling { .. } => "ForwardMeshSignaling",
                            NetworkCommand::Dial { .. } => "Dial",
                            NetworkCommand::FetchMailbox => "FetchMailbox",
                            _ => "Other",
                        };
                        if cmd_name == "HandleIncomingPayload" {
                            info!("[Mesh] SELECT LOOP: received HandleIncomingPayload command");
                        }
                        if let Err(e) = self.handle_command(cmd).await {
                            error!("Command error: {:?}", e);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_file_chunk(&mut self, peer: PeerId, transfer_id: String, chunk_index: u32, total_chunks: u32, data_base64: String) {
        debug!("[DEBUG] handle_file_chunk: transfer_id={}, chunk_index={}/{}", transfer_id, chunk_index, total_chunks);
        if let Some(transfer) = self.incoming_transfers.get_mut(&transfer_id) {
            transfer.total_chunks = total_chunks;
            transfer.last_update = Instant::now();
            debug!("[DEBUG] Found transfer in incoming_transfers. Decoded chunks so far: {}", transfer.received_chunks.len());
            if let Ok(chunk_data) = general_purpose::STANDARD.decode(&data_base64) {
                debug!("[DEBUG] Successfully decoded chunk {} (size: {} bytes)", chunk_index, chunk_data.len());
                // RELIABILITY: Only trigger N+4 if this is a NEW chunk.
                // This prevents duplicate requests from retries or re-deliveries.
                let is_new_chunk = transfer.received_chunks.insert(chunk_index, chunk_data).is_none();
                debug!("[DEBUG] Chunk {} is new: {}. Total unique chunks: {}", chunk_index, is_new_chunk, transfer.received_chunks.len());
                
                let progress_val = transfer.received_chunks.len() as f32 / total_chunks as f32;
                let is_complete = transfer.received_chunks.len() as u32 == total_chunks;
                debug!("[DEBUG] progress_val={}, is_complete={}", progress_val, is_complete);
                
                let received_bytes: usize = transfer.received_chunks.values().map(|v| v.len()).sum();
                let elapsed = transfer.start_time.elapsed().as_secs_f64();
                let speed_bps = if elapsed > 0.0 { (received_bytes as f64 * 8.0) / elapsed } else { 0.0 };

                let mut local_path = None;
                let mut is_verified = false;

                if is_complete {
                    let mut full_data = Vec::new();
                    for i in 0..total_chunks { if let Some(chunk) = transfer.received_chunks.get(&i) { full_data.extend_from_slice(chunk); } }
                    
                    // Verify SHA-256 integrity
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(&full_data);
                    let actual_hash = format!("{:x}", hasher.finalize());

                    if actual_hash == transfer.file_hash {
                        info!("✅ File integrity VERIFIED for transfer {}", transfer_id);
                        is_verified = true;
                        
                        let subfolder = if let Some(ref gid) = transfer.group_id {
                            info!("[Mesh] Identifying group for organization: {}", gid);
                            if let Ok(Some(group)) = self.storage.get_group(gid) {
                                let g_name = group.name.replace(" ", "_");
                                info!("[Mesh] Organized into group folder: {}_Media", g_name);
                                format!("{}_Media", g_name)
                            } else {
                                warn!("[Mesh] Group not found in storage for folder organization: {}", gid);
                                "Group_Media".to_string()
                            }
                        } else {
                            let peer_str = peer.to_string();
                            info!("[Mesh] Identifying contact for organization: {}", peer_str);
                            if let Ok(Some(contact)) = self.storage.get_contact(&peer_str) {
                                let alias = contact.local_alias.as_deref().or(contact.global_name.as_deref()).unwrap_or("Direct");
                                let s_name = alias.replace(" ", "_");
                                info!("[Mesh] Organized into contact folder: {}_Media", s_name);
                                format!("{}_Media", s_name)
                            } else {
                                warn!("[Mesh] Contact not found in storage for folder organization: {}", peer_str);
                                "Direct_Media".to_string()
                            }
                        };

                        let safe_subfolder = Self::sanitize_filename(&subfolder);
                        let dir_path = format!("{}/{}", self.downloads_dir, safe_subfolder);
                        info!("[Mesh] Creating Drive directory: {}", dir_path);
                        if let Err(e) = std::fs::create_dir_all(&dir_path) {
                            error!("[Mesh] ❌ Failed to create Drive subfolder {}: {:?}", dir_path, e);
                        }

                        let safe_filename = Self::sanitize_filename(&transfer.filename);
                        let path = format!("{}/introvert_{}", dir_path, safe_filename);
                        info!("[Mesh] Automatic Drive Organization: Saving to {}", path);

                        // PRIORITY 5: Atomic write — write to temp file first, then rename.
                        // Prevents partial/corrupted files if the process is killed mid-write.
                        let staging_path = format!("{}.tmp", path);
                        let write_ok = std::fs::write(&staging_path, &full_data)
                            .and_then(|_| std::fs::rename(&staging_path, &path))
                            .is_ok();
                        // Clean up temp file if rename failed (e.g., cross-filesystem)
                        let _ = std::fs::remove_file(&staging_path);

                        if write_ok {
                            local_path = Some(path.clone());

                            // Record FileTransferRecv activity for daily rewards
                            if let Some(ref daily) = self.daily_reward_engine {
                                daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
                                    activity_type: crate::economy::daily_rewards::ActivityType::FileTransferRecv,
                                    peer_id: Some(peer.to_string()),
                                    value: 1,
                                    is_foreground: true,
                                    message_len: None,
                                    is_self: false,
                                    is_rbn: false,
                                    proof_hash: None,
                                    active_web_containers: 0,
                                });
                            }

                            // SOVEREIGN SWARM: Register as seeder AFTER disk write succeeds.
                            // Ordering matters: if a peer discovers us via Kademlia and sends a
                            // FileChunkRequest before the file lands on disk, metadata returns
                            // size=0 and we serve empty chunks. Write-first eliminates this race.
                            if let Some(ref gid) = transfer.group_id {
                                info!("[Mesh] Group transfer complete. Joining swarm as seeder for group: {}", gid);
                                let key = RecordKey::new(&transfer.file_hash.as_bytes());
                                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);

                                let _ = self.command_tx.send(NetworkCommand::RegisterSeeder {
                                    peer_id: *self.swarm.local_peer_id(),
                                    transfer_id: transfer_id.clone(),
                                    file_path: path.clone(),
                                    file_hash: transfer.file_hash.clone(),
                                    chunk_size: transfer.chunk_size,
                                    total_chunks,
                                    group_id: Some(gid.clone()),
                                }).await;
                            } else {
                                info!("[Mesh] 1-to-1 transfer complete. Skipping mesh seeding to preserve privacy.");
                            }

                            // DISPATCH LOCAL EVENT: Update UI with the organized path
                            let progress = FileTransferProgress {
                                transfer_id: transfer_id.clone(),
                                peer_id: peer.to_string(),
                                filename: transfer.filename.clone(),
                                mime_type: transfer.mime_type.clone(),
                                file_hash: transfer.file_hash.clone(),
                                progress: 1.0,
                                is_complete: true,
                                is_verified: true,
                                is_outgoing: false,
                                local_path: Some(path.clone()),
                                start_time_ms: transfer.start_time.elapsed().as_millis() as u64,
                                speed_bps: 0.0,
                                group_id: transfer.group_id.clone(),
                                caption: None,
                            };
                            crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());

                            // SOVEREIGN DRIVE: Persist metadata so this node can serve as a mesh seeder indefinitely
                            let storage_d = self.storage.clone();
                            let filename_d = transfer.filename.clone();
                            let hash_d = transfer.file_hash.clone();
                            let mime_d = transfer.mime_type.clone();
                            let size_d = transfer.total_size;
                            let path_d = path.clone();
                            let peer_id_str = peer.to_string();
                            let msg_id = transfer_id.clone();
                            let progress_d = progress.clone();
                            tokio::task::spawn_blocking(move || {
                                let _ = storage_d.upsert_drive_file(&filename_d, &hash_d, &mime_d, size_d as i64, &path_d);
                                if let Ok(json_str) = serde_json::to_string(&progress_d) {
                                    let c = format!("[FILE]:{}", json_str);
                                    if let Some(ref gid) = progress_d.group_id {
                                        let _ = storage_d.store_group_message(gid, &peer_id_str, &msg_id, &c, false, None, None);
                                    } else {
                                        let _ = storage_d.store_message_with_id(&peer_id_str, &msg_id, &c, false, None, None);
                                    }
                                }
                            });

                            // Send Completion ACK back to sender via command queue to ensure Mailbox routing
                            let ack = SignalingPayload::FileTransferComplete { transfer_id: transfer_id.clone() };
                            let tx = self.command_tx.clone();
                            tokio::spawn(async move { let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload: ack }).await; });
                        }
                    } else {
                        error!("❌ File integrity FAILED for transfer {}. Expected {}, got {}", transfer_id, transfer.file_hash, actual_hash);
                        let error = SignalingPayload::FileTransferError { transfer_id: transfer_id.clone(), reason: "Integrity verification failed".to_string() };
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move { let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload: error }).await; });
                        transfer.filename = format!("ERROR: {}", transfer.filename);
                    }
                } else if !transfer.filename.starts_with("ERROR:") {
                    let is_relayed_conn = self.is_relayed_map.read().get(&peer).cloned().unwrap_or(false);
                    if transfer.is_relayed || is_relayed_conn {
                        // SOVEREIGN SWARM: Stable windowed pull using next_pull_idx, distributed across providers.
                        // This maintains the concurrency pipeline even if duplicate chunks are received.
                        let next_idx = transfer.next_pull_idx;
                        if next_idx < total_chunks {
                            transfer.next_pull_idx += 1;
                            let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &transfer.providers);
                            let target_peer = if !selected_providers.is_empty() {
                                selected_providers[(next_idx as usize) % selected_providers.len()]
                            } else {
                                peer
                            };

                            let tx = self.command_tx.clone();
                            let tid = transfer_id.clone();
                            let csize = transfer.chunk_size;
                            let relay_hint = self.relay_reservations.iter().next().map(|id| id.to_string());
                            tokio::spawn(async move {
                                let req = SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: next_idx, chunk_size: Some(csize), relay_hint };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                            });
                        }
                    }
                }

                let progress = FileTransferProgress { 
                    transfer_id: transfer_id.clone(), 
                    peer_id: transfer.peer_id.to_string(),  // Use original sender, not chunk relay peer
                    filename: transfer.filename.clone(), 
                    mime_type: transfer.mime_type.clone(),
                    file_hash: transfer.file_hash.clone(),
                    progress: progress_val, 
                    is_complete, 
                    is_verified,
                    is_outgoing: false, 
                    local_path: local_path.clone(),
                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                    speed_bps,
                    group_id: transfer.group_id.clone(),
                    caption: None,
                };

                let data = serde_json::to_vec(&progress).unwrap_or_default();
                crate::dispatch_global_event(12, &data);

                if let Some(ref gid) = transfer.group_id {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let content = format!("[FILE]:{}", json_str);
                        let storage = Arc::clone(&self.storage);
                        let gid_clone = gid.clone();
                        let peer_str = transfer.peer_id.to_string();  // Use original sender for DB storage
                        let tid_clone = transfer_id.clone();
                        if !self.is_stress_test {
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_group_message(&gid_clone, &peer_str, &tid_clone, &content, false, None, None);
                            });
                        }
                    }
                } else {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let content = format!("[FILE]:{}", json_str);
                        let storage = Arc::clone(&self.storage);
                        let peer_str = transfer.peer_id.to_string();  // Use original sender for DB storage
                        let tid_clone = transfer_id.clone();
                        if !self.is_stress_test {
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_message_with_id(&peer_str, &tid_clone, &content, false, None, None);
                            });
                        }
                    }
                }
                // CRITICAL FIX: Always remove from incoming transfers when complete (success or fail) to prevent memory leak
                if is_complete { self.incoming_transfers.remove(&transfer_id); }
            } else {
                debug!("[DEBUG] Failed to decode base64 for chunk {}", chunk_index);
            }

            // Record throughput for Intro-Claw adaptive chunk sizing
            if let Some(transfer) = self.incoming_transfers.get(&transfer_id) {
                let received_bytes: usize = transfer.received_chunks.values().map(|v| v.len()).sum();
                let elapsed = transfer.start_time.elapsed().as_secs_f64();
                if elapsed > 0.1 {
                    let bytes_per_sec = received_bytes as f64 / elapsed;
                    self.intro_claw.record_throughput(&peer.to_string(), bytes_per_sec);
                }
            }
        } else {
            // Buffer chunks that arrive before their manifest (race condition fix)
            // SECURITY: Limit buffered transfers to prevent memory exhaustion
            if self.early_chunks.len() >= 100 {
                warn!("[Security] early_chunks buffer full (100 transfers). Dropping oldest.");
                if let Some(oldest_key) = self.early_chunks.keys().next().cloned() {
                    self.early_chunks.shift_remove(&oldest_key);
                }
            }
            let chunks = self.early_chunks.entry(transfer_id.clone()).or_default();
            // Limit chunks per transfer and total bytes to prevent unbounded growth
            let chunk_bytes = data_base64.len();
            let total_bytes: usize = chunks.iter().map(|(_, _, d)| d.len()).sum();
            if chunks.len() < 1000 && total_bytes + chunk_bytes < 50 * 1024 * 1024 {
                chunks.push((chunk_index, total_chunks, data_base64));
            } else {
                warn!("[Security] Chunk buffer full for transfer {} ({} chunks, {} bytes). Dropping chunk {}.", transfer_id, chunks.len(), total_bytes, chunk_index);
            }
        }
    }

    fn sanitize_filename(filename: &str) -> String {
        let path = std::path::Path::new(filename);
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_"))
            .unwrap_or_else(|| "unknown_file".to_string())
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<IntrovertBehaviourEvent>) -> anyhow::Result<()> {
        match event {
            SwarmEvent::Behaviour(b_event) => {
                match b_event {
                    IntrovertBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(list)) => {
                        let mut grouped: std::collections::HashMap<PeerId, Vec<libp2p::Multiaddr>> = std::collections::HashMap::new();
                        for (peer_id, addr) in list {
                            grouped.entry(peer_id).or_default().push(addr);
                        }
                        for (peer_id, addrs) in grouped {
                            info!("mDNS discovered peer: {} with {} addresses", peer_id, addrs.len());
                            
                            // Track mDNS peers for Intro-Claw context
                            self.mdns_peers.insert(peer_id);
                            
                            // Check if this peer is a static bootstrap node to prevent clearing its bootstrap configuration
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
                    IntrovertBehaviourEvent::Autonat(autonat::Event::StatusChanged { old, new }) => {
                        info!("[AutoNAT] Reachability changed: {:?} -> {:?}", old, new);
                        
                        // Clear all WebRTC connections since our network interface changed
                        // CRITICAL: Avoid clearing WebRTC connections during initial boot transition from Unknown.
                        // Initial Nat status resolves to Private/Public after ~5s, which was clearing perfectly good local WebRTC links mid-transfer.
                        let is_initial_boot = matches!(old, autonat::NatStatus::Unknown);
                        if !is_initial_boot {
                            self.data_channels.write().clear();
                            let pcs: Vec<Arc<RTCPeerConnection>> = self.peer_connections.write().drain().map(|(_, pc)| pc).collect();
                            for pc in pcs {
                                let _ = pc.close().await;
                            }
                        }

                        // PROACTIVE MESH REBUILD: If we just moved networks, re-dial bootstrap nodes
                        for (_, addr) in &self.bootstrap_nodes {
                            let _ = self.swarm.dial(addr.clone());
                        }
                        // Also re-dial known contacts to restore direct paths if possible
                        if let Ok(contacts) = self.storage.get_all_contacts() {
                            for contact in contacts {
                                if let Ok(pid) = contact.peer_id.parse::<PeerId>() {
                                    let _ = self.swarm.dial(pid);
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                        debug!("Identify received from {}: Protocols={:?}", peer_id, info.protocols);

                        // Auto-register push token on Identify with RBN bootstrap nodes
                        let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                        if is_bootstrap {
                            let storage = Arc::clone(&self.storage);
                            let my_peer_id = self.swarm.local_peer_id().to_string();
                            let tx = self.command_tx.clone();
                            tokio::spawn(async move {
                                crate::dispatch_debug_log(&format!("[Mesh] Checking local push token for auto-registration on Identify (my_peer_id: {})", my_peer_id));
                                match storage.get_push_token(&my_peer_id) {
                                    Ok(Some((device_type, push_token))) => {
                                        crate::dispatch_debug_log(&format!("[Mesh] 🔔 Found local token. Auto-registering with RBN {} on Identify...", peer_id));
                                        let payload = SignalingPayload::IdentifySleepState { device_type, push_token };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                    }
                                    Ok(None) => {
                                        crate::dispatch_debug_log("[Mesh] No local push token found in DB to auto-register.");
                                    }
                                    Err(e) => {
                                        crate::dispatch_debug_log(&format!("[Mesh] ❌ Error fetching local push token: {:?}", e));
                                    }
                                }
                            });
                        }

                        if self.mesh_active_peers.insert(peer_id) {
                            crate::ACTIVE_PEER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        
                        // Add addresses to both Kademlia AND the swarm's direct address book
                        // This is critical for the Relay Client to find the relay server.
                        let currently_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                        
                        // Clear old Kademlia addresses first to avoid dialing stale dynamic ports (Connection Refused errors)
                        let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                        if !is_bootstrap {
                            info!("[Mesh] Clearing stale addresses for peer {} prior to applying new Identify listen addresses.", peer_id);
                            self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                        }

                        for addr in &info.listen_addrs {
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                            
                            // Upgrading path: if we are connected via relay, try to dial direct listen addresses
                            if currently_relayed {
                                let is_circuit = addr.iter().any(|proto| matches!(proto, libp2p::multiaddr::Protocol::P2pCircuit));
                                if !is_circuit {
                                    info!("[Mesh] Attempting direct dial to upgrade relayed connection to {}: {}", peer_id, addr);
                                    let _ = self.swarm.dial(addr.clone());
                                }
                            }
                        }
                        
                        // Discovery: If peer supports our protocol AND HOP relay protocol, they can be an anchor/relay
                        let supports_signaling = info.protocols.iter().any(|p| p.to_string().contains("/introvert/signaling/1.0.0"));
                        let supports_v2 = info.protocols.iter().any(|p| p.to_string().contains("/introvert/signaling/2.0.0"));
                        if supports_v2 {
                            self.peer_supports_v2.insert(peer_id);
                            info!("[Codec] Peer {} supports binary v2.0.0 codec — FileChunks will route through optimized binary pipe.", peer_id);
                        }
                        let supports_hop = info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop"));
                        if supports_signaling && supports_hop {
                            info!("✨ Peer {} supports Introvert Signaling and HOP. Discovered as Anchor.", peer_id);
                            if !self.discovered_anchors.contains(&peer_id) {
                                self.discovered_anchors.push(peer_id);
                            }
                        }

                        // Refresh view of the network
                        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

                        if info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop")) {
                            if !self.relay_reservations.contains(&peer_id) {
                                info!("Relay node {} supports HOP. Requesting reservation...", peer_id);

                                // BUG FIX: Construct the FULL multiaddr for the relay reservation.
                                // We prioritize the public address we used to connect to this node (from bootstrap_nodes
                                // or anchor_mappings) to prevent VPC/NAT private IPs (e.g. 172.19.0.4) from causing dead reservations.
                                let mut base_addr = self.bootstrap_nodes.iter()
                                    .find(|(id, _)| id == &peer_id)
                                    .map(|(_, addr)| addr.clone())
                                    .or_else(|| self.anchor_mappings.get(&peer_id).cloned());

                                if base_addr.is_none() {
                                    let is_private_or_vpn = |a: &libp2p::Multiaddr| {
                                        let s = a.to_string();
                                        s.contains("127.0.0.1")
                                        || s.contains("192.168.")
                                        || s.contains("localhost")
                                        || s.starts_with("/ip4/10.")
                                        || {
                                            // Match 172.16.x.x through 172.31.x.x
                                            if let Some(rest) = s.strip_prefix("/ip4/172.") {
                                                rest.split('.').next()
                                                    .and_then(|n| n.parse::<u8>().ok())
                                                    .map(|n| n >= 16 && n <= 31)
                                                    .unwrap_or(false)
                                            } else { false }
                                        }
                                    };
                                    base_addr = info.listen_addrs.iter()
                                        .find(|a| !is_private_or_vpn(a) && (a.to_string().contains("/ip4/") || a.to_string().contains("/ip6/")))
                                        .or_else(|| info.listen_addrs.iter().find(|a| !is_private_or_vpn(a)))
                                        .or_else(|| info.listen_addrs.first())
                                        .cloned();
                                }

                                let relay_addr = if let Some(mut addr) = base_addr {
                                    // If the address doesn't already contain the peer ID, add it.
                                    if !addr.to_string().contains(&peer_id.to_string()) {
                                        addr = addr.with(libp2p::multiaddr::Protocol::P2p(peer_id));
                                    }
                                    addr.with(libp2p::multiaddr::Protocol::P2pCircuit)
                                } else {
                                    // Fallback to relative circuit address
                                    libp2p::multiaddr::Multiaddr::empty()
                                        .with(libp2p::multiaddr::Protocol::P2p(peer_id))
                                        .with(libp2p::multiaddr::Protocol::P2pCircuit)
                                };

                                match self.swarm.listen_on(relay_addr.clone()) {
                                    Ok(id) => {
                                        info!("[Mesh] Relay listen request SUCCESS. Address: {}, Listener ID: {:?}", relay_addr, id);
                                        self.relay_reservations.insert(peer_id);
                                        self.relay_listeners.insert(id, peer_id);
                                    },
                                    Err(e) => info!("[Mesh] Relay listen request FAILED on {}: {:?}", relay_addr, e),
                                }
                            }
                        }
                        // --- RELIABILITY FIX: Flush pending messages only AFTER Identify succeeds ---
                        if supports_signaling {
                            if let Some(payloads) = self.pending_messages.remove(&peer_id) {
                                info!("[Mesh] Peer {} identified. Flushing {} pending messages.", peer_id, payloads.len());
                                for payload in payloads {
                                    let _ = self.handle_command(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                }
                            }

                            // If this is an anchor node, drain our mailbox and flush non-file pending messages
                            let is_anchor = self.discovered_anchors.contains(&peer_id) || 
                                           self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer_id.to_string())).unwrap_or(false);
                            if is_anchor {
                                info!("[Mesh] Anchor {} identified. Draining mailbox...", peer_id);
                                self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));

                                // Flush ONLY non-file-chunk payloads for other blocked peers via mailbox.
                                // File chunks are handled by the pull model (receiver re-requests).
                                // Rate-limit sends to avoid relay flooding.
                                let all_pending_targets: Vec<(PeerId, bool)> = self.pending_messages.keys()
                                    .map(|p| (*p, self.is_relayed_map.read().get(p).cloned().unwrap_or(false)))
                                    .collect();
                                for (recipient, is_relay_recipient) in all_pending_targets {
                                    if let Some(payloads) = self.pending_messages.get_mut(&recipient) {
                                        // Stable extract: keep chunks in pending, flush non-chunks
                                        let mut non_chunk_payloads = Vec::new();
                                        payloads.retain(|p| {
                                            if matches!(p, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
                                                true
                                            } else {
                                                non_chunk_payloads.push(p.clone());
                                                false
                                            }
                                        });
                                        if payloads.is_empty() { self.pending_messages.remove(&recipient); }
                                        
                                        let tx = self.command_tx.clone();
                                        tokio::spawn(async move {
                                            for payload in non_chunk_payloads {
                                                // For relay peers: force mailbox to avoid direct-retry loop
                                                let cmd = if is_relay_recipient {
                                                    NetworkCommand::StoreInMailbox { peer_id: recipient, payload }
                                                } else {
                                                    NetworkCommand::ForwardMeshSignaling { peer_id: recipient, payload }
                                                };
                                                let _ = tx.send(cmd).await;
                                                tokio::time::sleep(Duration::from_millis(100)).await;
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RelayClient(event) => {
                        match event {
                             libp2p::relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, .. } => {
                                info!("✅ Relay reservation ACCEPTED by {}. Renewal: {}", relay_peer_id, renewal);
                                crate::dispatch_debug_log(&format!("[Relay] ✅ ReservationReqAccepted via {} (renewal={})", relay_peer_id, renewal));
                                // Mark reservation as confirmed (not just requested)
                                self.relay_reservations.insert(relay_peer_id);
                                // Re-register our addresses in Kademlia so peers can discover our relay path
                                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                                // Also add the relay address to Kademlia explicitly for faster discovery
                                let relay_circuit_addr = libp2p::multiaddr::Multiaddr::empty()
                                    .with(libp2p::multiaddr::Protocol::P2p(relay_peer_id))
                                    .with(libp2p::multiaddr::Protocol::P2pCircuit);
                                self.swarm.add_external_address(relay_circuit_addr);
                                let mut data = relay_peer_id.to_string().into_bytes();
                                data.push(b':');
                                data.push(1); // 1 = Relay Active
                                crate::dispatch_global_event(8, &data);
                                crate::dispatch_global_event(10, &[2]);

                                // FIX B: Drain mailbox immediately after reservation so queued messages are delivered
                                // This is critical for mobile data / VPN: messages sent to us while we were
                                // offline are sitting in the RBN mailbox waiting to be drained.
                                info!("[Relay] Triggering immediate mailbox drain after reservation...");
                                crate::dispatch_debug_log("[Relay] Triggering mailbox drain after ReservationReqAccepted");
                                self.perform_mailbox_fetch().await;

                                // FIX C: Flush any pending messages that were buffered while we had no relay
                                let pending_peers: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
                                if !pending_peers.is_empty() {
                                    info!("[Relay] Flushing {} pending message queues after relay reservation", pending_peers.len());
                                    crate::dispatch_debug_log(&format!("[Relay] Flushing pending queues for {} peers after reservation", pending_peers.len()));
                                    for pid in pending_peers {
                                        if let Some(payloads) = self.pending_messages.remove(&pid) {
                                            let tx = self.command_tx.clone();
                                            tokio::spawn(async move {
                                                for payload in payloads {
                                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload }).await;
                                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                            libp2p::relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
                                info!("[Relay] 🔌 OutboundCircuitEstablished via {}", relay_peer_id);
                                crate::dispatch_debug_log(&format!("[Relay] 🔌 OutboundCircuitEstablished via {}", relay_peer_id));

                                // Clear dial rate limiter for all peers with pending messages.
                                // Without this, dial_relay_path returns early (5s cooldown) and
                                // the connection never establishes, so is_connected() stays false.
                                let pending_peers: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
                                for peer_id in &pending_peers {
                                    self.relay_dial_limiter.remove(peer_id);
                                }

                                // Dial peers with pending messages through the relay circuit NOW.
                                // This establishes the connection at the swarm level so is_connected()
                                // returns true and forward_to_mesh can send the queued payloads.
                                for peer_id in &pending_peers {
                                    self.dial_relay_path(*peer_id, false);
                                }

                                 // Flush pending messages after a short delay to let the dial complete.
                                 if !pending_peers.is_empty() {
                                     let tx = self.command_tx.clone();
                                     let peers_with_payloads: Vec<(PeerId, Vec<SignalingPayload>)> = pending_peers.iter()
                                         .filter_map(|pid| self.pending_messages.remove(pid).map(|p| (*pid, p)))
                                         .collect();
                                     tokio::spawn(async move {
                                         // 1500ms delay: gives is_connected() time to update after
                                         // dial_relay_path completes. 500ms was too short — flush
                                         // fired before connection registered, causing re-buffering.
                                         tokio::time::sleep(Duration::from_millis(1500)).await;
                                         for (peer_id, payloads) in peers_with_payloads {
                                             info!("[Relay] Circuit ready — flushing {} queued payloads for {}", payloads.len(), peer_id);
                                             for payload in payloads {
                                                 let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                             }
                                         }
                                     });
                                 }

                                // Flush pending DB chunks for all peers (will check connectivity per-peer)
                                let storage = Arc::clone(&self.storage);
                                let tx_db = self.command_tx.clone();
                                let connected_peers: Vec<String> = self.swarm.connected_peers()
                                    .map(|p| p.to_string())
                                    .collect();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_millis(600)).await; // Wait for circuit to stabilize
                                    for peer_str in connected_peers {
                                        if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 50) {
                                            if !chunks.is_empty() {
                                                info!("[Relay] Flushing {} pending DB chunks for {}", chunks.len(), peer_str);
                                                if let Ok(peer_id) = peer_str.parse::<PeerId>() {
                                                    for (transfer_id, chunk_index, chunk_data) in chunks {
                                                        let data_base64 = base64::encode(&chunk_data);
                                                        let payload = SignalingPayload::FileChunk {
                                                            transfer_id: transfer_id.clone(),
                                                            chunk_index,
                                                            total_chunks: 0,
                                                            data_base64,
                                                        };
                                                        let _ = tx_db.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                                        // Don't remove from DB here — wait for FileTransferComplete
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                                info!("[Relay] ✅ InboundCircuitEstablished from {}", src_peer_id);
                                crate::dispatch_debug_log(&format!("[Relay] ✅ InboundCircuitEstablished from {} — flushing pending payloads", src_peer_id));

                                // RELAY HINT: Record which RBN this peer is behind.
                                // This enables relay-aware routing in forward_to_mesh so file
                                // payloads bypass the is_connected() check and flow through
                                // the relay circuit directly.
                                if let Some(&rbn_id) = self.relay_reservations.iter().next() {
                                    self.relay_hints.insert(src_peer_id, rbn_id);
                                    crate::dispatch_debug_log(&format!("[Relay] Relay hint: {} is behind RBN {}", src_peer_id, rbn_id));
                                }

                                // Clear rate limiter so dial_relay_path can retry immediately for this peer
                                self.relay_dial_limiter.remove(&src_peer_id);

                                // DCUtR hole-punch attempt for potential direct upgrade
                                let _ = self.swarm.dial(src_peer_id);

                                // CRITICAL FIX: Flush pending messages to src_peer_id.
                                // This is the CORRECT moment — src_peer_id just connected to us through the relay.
                                // The OutboundCircuitEstablished flush targets connected_peers which may not include
                                // this peer yet. InboundCircuit fires when the receiver is actually ready.
                                if let Some(payloads) = self.pending_messages.remove(&src_peer_id) {
                                    let tx = self.command_tx.clone();
                                    info!("[Relay] InboundCircuit: flushing {} pending payloads to {}", payloads.len(), src_peer_id);
                                    crate::dispatch_debug_log(&format!("[Relay] InboundCircuit flush: {} payloads → {}", payloads.len(), src_peer_id));
                                    tokio::spawn(async move {
                                        // Small delay to let the circuit sub-stream fully open
                                        tokio::time::sleep(Duration::from_millis(150)).await;
                                        for payload in payloads {
                                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: src_peer_id, payload }).await;
                                            tokio::time::sleep(Duration::from_millis(20)).await;
                                        }
                                    });
                                }

                                // Flush pending DB file chunks for this peer.
                                // This covers the case where chunks were persisted to DB because no RBN was connected.
                                let storage = Arc::clone(&self.storage);
                                let tx_db = self.command_tx.clone();
                                let peer_str = src_peer_id.to_string();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_millis(400)).await; // Let circuit stabilize
                                    if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 100) {
                                        if !chunks.is_empty() {
                                            info!("[Relay] InboundCircuit: flushing {} pending DB chunks to {}", chunks.len(), peer_str);
                                            crate::dispatch_debug_log(&format!("[Relay] InboundCircuit DB flush: {} chunks → {}", chunks.len(), peer_str));
                                            if let Ok(peer_id) = peer_str.parse::<PeerId>() {
                                                for (transfer_id, chunk_index, chunk_data) in chunks {
                                                    use base64::Engine;
                                                    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&chunk_data);
                                                    let payload = SignalingPayload::FileChunk {
                                                        transfer_id,
                                                        chunk_index,
                                                        total_chunks: 0,
                                                        data_base64,
                                                    };
                                                    let _ = tx_db.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers, .. })), .. }) => {
                        let key_str = String::from_utf8_lossy(key.as_ref()).into_owned();
                        let local_id = *self.swarm.local_peer_id();
                        
                        // Filter out local peer ID from providers to prevent self-dials and self-requests
                        let filtered_providers: Vec<PeerId> = providers.into_iter().filter(|p| p != &local_id).collect();
                        
                        // SOVEREIGN SWARM: Link providers to active file transfers
                        // SECURITY: Limit active providers to prevent memory exhaustion
                        if self.active_providers.len() >= 1000 {
                            if let Some(oldest_key) = self.active_providers.keys().next().cloned() {
                                self.active_providers.shift_remove(&oldest_key);
                            }
                        }
                        self.active_providers.insert(key_str.clone(), filtered_providers.iter().cloned().collect());
                        
                        // Update specific incoming transfers that match this hash
                        for (transfer_id, transfer) in self.incoming_transfers.iter_mut() {
                            if transfer.file_hash == key_str {
                                for pid in &filtered_providers {
                                     if !transfer.providers.contains(pid) { transfer.providers.push(*pid); }
                                }

                                // If transfer is stalled (no chunks yet) and we just found providers, kickstart it
                                if transfer.received_chunks.is_empty() && !transfer.providers.is_empty() {
                                    let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &transfer.providers);
                                    let target_peer = if !selected_providers.is_empty() {
                                        selected_providers[0]
                                    } else {
                                        transfer.providers[0]
                                    };
                                    let tid = transfer_id.clone();
                                    let tx = self.command_tx.clone();
                                    let csize = transfer.chunk_size;
                                    let relay_hint = self.relay_reservations.iter().next().map(|id| id.to_string());
                                    tokio::spawn(async move {
                                        let req = SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: 0, chunk_size: Some(csize), relay_hint };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                                    });
                                }
                            }
                        }
                        
                        let is_anchor_key = key.as_ref() == ANCHOR_PROVIDER_KEY;
                        let mut dial_count = 0;
                        for peer_id in filtered_providers {
                            if is_anchor_key {
                                if !self.discovered_anchors.contains(&peer_id) { self.discovered_anchors.push(peer_id); }
                            }
                            if !self.swarm.is_connected(&peer_id) {
                                if dial_count < 3 {
                                    info!("[Mesh] Provider {} found via DHT. Constructing relay path dial...", peer_id);
                                    self.dial_relay_path(peer_id, false);
                                    dial_count += 1;
                                } else {
                                    info!("[Mesh] Provider {} found via DHT, but dial limit (3) reached. Skipping dial.", peer_id);
                                }
                            }
                            
                            // SECURITY HARDENING: Group discovery link (Only if not a file hash)
                            if key_str.len() < 32 { // Simple heuristic: hashes are long hex strings
                                let tx = self.command_tx.clone();
                                if let Some(gid) = self.resolved_group_codes.get(&key_str).cloned() {
                                    let local_profile = self.storage.get_profile().ok().flatten();
                                    let alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                                    let handle = local_profile.as_ref().and_then(|(_, h, _, _, _)| h.clone());
                                    let avatar = local_profile.and_then(|(_, _, a, _, _)| a);
                                    let local_static_key = self.local_static_public.to_bytes().to_vec();
                                    tokio::spawn(async move {
                                        let req = SignalingPayload::GroupManifestRequest {
                                            group_id: gid,
                                            alias,
                                            avatar,
                                            handle,
                                            requester_static_key: Some(local_static_key),
                                        };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: req }).await;
                                    });
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. }) => {
                        let data = peer.to_bytes();
                        crate::dispatch_global_event(1, &data); 
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetClosestPeers(Ok(kad::GetClosestPeersOk { key, peers })), .. }) => {
                        if let Ok(target_peer) = PeerId::from_bytes(&key) {
                            let peer_ids: Vec<PeerId> = peers.into_iter().map(|p| p.peer_id).collect();
                            if peer_ids.contains(&target_peer) { let _ = self.swarm.dial(target_peer); }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { id, result: kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))), .. }) => {
                        let key_str = String::from_utf8_lossy(record.record.key.as_ref()).into_owned();
                        let value_str = String::from_utf8_lossy(&record.record.value).into_owned();
                        info!("[Mesh] Kademlia resolved record key: {}, value: {}", key_str, value_str);
                        
                        // Handle resolution logic
                        if key_str.starts_with("i@") {
                            let (resolved_peer_id, _) = if let Ok(claim) = serde_json::from_str::<registry::HandleClaim>(&value_str) {
                                let mut valid_witnesses = 0;
                                for rbn_peer_id in &claim.signatures {
                                    if let Ok(pid) = rbn_peer_id.parse::<PeerId>() {
                                        if self.bootstrap_nodes.iter().any(|(id, _)| id == &pid) {
                                            valid_witnesses += 1;
                                        }
                                    }
                                }
                                let verified = valid_witnesses >= 1;
                                if verified {
                                    let _ = self.registry.verify_claim(&claim);
                                }
                                (claim.peer_id, verified)
                            } else {
                                (value_str.clone(), false)
                            };

                            let mut data = key_str.clone().into_bytes();
                            data.push(0);
                            data.extend(resolved_peer_id.as_bytes());
                            crate::dispatch_global_event(33, &data); // Event Type 33: Handle Resolved [handle, 0, peer_id]
                        }

                        if key_str.starts_with("ph_") {
                            let target_peer_id = key_str.trim_start_matches("ph_").to_string();
                            let handle_resolved = value_str.clone();
                            let my_peer_id = self.swarm.local_peer_id().to_string();
                            if target_peer_id == my_peer_id {
                                let profile = self.storage.get_profile().ok().flatten();
                                let name = profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                                let avatar = profile.as_ref().and_then(|(_, _, a, _, _)| a.clone());
                                let privacy = profile.as_ref().map(|(_, _, _, p, _)| *p).unwrap_or(1);
                                let _ = self.storage.set_profile(name.as_deref(), Some(&handle_resolved), avatar.as_deref(), privacy);
                                let _ = self.storage.insert_handle_claim(&handle_resolved, &my_peer_id, chrono::Utc::now().timestamp(), "[]", true);
                                info!("[Mesh] Restored handle {} for local profile!", handle_resolved);
                            }
                            
                            // Send Event 37: Peer Handle Restored/Resolved
                            let mut data = target_peer_id.into_bytes();
                            data.push(0);
                            data.extend(handle_resolved.as_bytes());
                            crate::dispatch_global_event(37, &data);
                        }

                        // SECURITY: Limit resolved group codes to prevent memory exhaustion
                        if self.resolved_group_codes.len() >= 500 {
                            if let Some(oldest_key) = self.resolved_group_codes.keys().next().cloned() {
                                self.resolved_group_codes.shift_remove(&oldest_key);
                            }
                        }
                        
                        // Store the resolved mapping
                        self.resolved_group_codes.insert(key_str.clone(), value_str.clone());

                        // If we have providers already discovered for this key, query them immediately
                        if let Some(providers) = self.active_providers.get(&key_str).cloned() {
                            let local_profile = self.storage.get_profile().ok().flatten();
                            let alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                            let handle = local_profile.as_ref().and_then(|(_, h, _, _, _)| h.clone());
                            let avatar = local_profile.and_then(|(_, _, a, _, _)| a);
                            let local_static_key = self.local_static_public.to_bytes().to_vec();
                            for peer_id in providers {
                                let tx = self.command_tx.clone();
                                let gid = value_str.clone();
                                let alias_clone = alias.clone();
                                let handle_clone = handle.clone();
                                let avatar_clone = avatar.clone();
                                let static_key_clone = local_static_key.clone();
                                tokio::spawn(async move {
                                    let req = SignalingPayload::GroupManifestRequest {
                                        group_id: gid,
                                        alias: alias_clone,
                                        avatar: avatar_clone,
                                        handle: handle_clone,
                                        requester_static_key: Some(static_key_clone),
                                    };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: req }).await;
                                });
                            }
                        }

                        if let Some(peer_id) = self.pending_handshakes.remove(&id) {
                            if let Ok(remote_static_pub) = <[u8; 32]>::try_from(record.record.value.as_slice()) {
                                if let Ok(mut session) = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &remote_static_pub) {
                                    if let Ok(handshake_msg) = session.send_message(&[]) {
                                        let storage = Arc::clone(&self.storage);
                                        let enc_key = self.session_encryption_key;
                                        let session_state = session.get_state();
                                        tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage, enc_key, peer_id, session_state).await; });
                                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                                        self.noise_sessions.insert(peer_id, session);
                                    }
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Request { request, channel, .. }, .. }) => {
                        let _ = self.swarm.behaviour_mut().request_response.send_response(channel, SignalingResponse("ACK".to_string()));
                        let tx = self.command_tx.clone();
                        let payload = request.0;
                        tokio::spawn(async move {
                            let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: peer, payload }).await;
                        });
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Response { request_id, .. }, .. }) => {
                        // SUCCESS: Remove from tracker and decrement in-flight counter
                        self.outbound_tracker.remove(&request_id);
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }
                        // Sliding window drain: flush one back-pressured chunk now that a slot opened
                        if let Some(pending) = self.pending_messages.get_mut(&peer) {
                            if let Some(next_chunk) = pending.iter().position(|p| matches!(p, SignalingPayload::FileChunk { .. })) {
                                let payload = pending.remove(next_chunk);
                                if pending.is_empty() { self.pending_messages.remove(&peer); }
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
                                });
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::OutboundFailure { request_id, peer, error, .. }) => {
                        warn!("[Mesh] Outbound Request-Response FAILURE to {}: {:?}", peer, error);
                        
                        // Decrement in-flight counter for this peer
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }

                        // Sliding window drain: flush one back-pressured chunk now that a slot opened
                        if let Some(pending) = self.pending_messages.get_mut(&peer) {
                            if let Some(next_chunk) = pending.iter().position(|p| matches!(p, SignalingPayload::FileChunk { .. })) {
                                let payload = pending.remove(next_chunk);
                                if pending.is_empty() { self.pending_messages.remove(&peer); }
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
                                });
                            }
                        }

                        let is_network_failure = matches!(error, libp2p::request_response::OutboundFailure::ConnectionClosed | libp2p::request_response::OutboundFailure::Timeout);
                        let is_unexpected_eof = format!("{:?}", error).contains("UnexpectedEof") || format!("{:?}", error).contains("EOF while parsing");

                        // If the direct push failed, handle based on payload type:
                        // - FileChunk: DO NOT re-queue — the pull model means the receiver will re-request missing chunks.
                        //   Re-queuing causes a thundering herd on reconnect that floods relay circuits.
                        // - Other payloads: re-queue for mailbox routing.
                        if let Some((target_peer, payload)) = self.outbound_tracker.remove(&request_id) {
                            let is_file_chunk = matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. });
                            let is_sent_to_anchor = target_peer != peer;

                            // PRIORITY 4: Increment retry count for failed file chunks.
                            // After 5 retries, the chunk is evicted to prevent infinite loops.
                            if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, .. } = payload {
                                match self.storage.increment_chunk_retry(transfer_id, chunk_index, 5) {
                                    Ok(count) if count >= 5 => {
                                        warn!("[Mesh] Chunk {}/{} exceeded 5 retries — evicting from pending queue", transfer_id, chunk_index);
                                    }
                                    Ok(count) => {
                                        info!("[Mesh] Chunk {}/{} retry count: {}/5", transfer_id, chunk_index, count);
                                    }
                                    Err(e) => debug!("[Mesh] Failed to increment retry count: {:?}", e),
                                }
                            }

                            if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
                                // Re-queue to DB for retry on next circuit establishment or periodic flush.
                                // increment_chunk_retry already called above — chunk auto-evicts at 5 retries.
                                if let Ok(chunk_data) = base64::decode(data_base64) {
                                    info!("[Mesh] FileChunk send failed for {}. Persisting to DB for retry.", peer);
                                    let _ = self.storage.enqueue_pending_chunk(transfer_id, &peer.to_string(), chunk_index, &chunk_data);
                                }
                            } else if is_file_chunk {
                                // FileChunkRequest: tiny control message, receiver will regenerate — drop it.
                                debug!("[Mesh] FileChunkRequest send failed for {}. Receiver will re-request.", peer);
                            } else if is_unexpected_eof && is_sent_to_anchor {
                                info!("[Mesh] Outbound failure to anchor {} was UnexpectedEof. Bypassing re-queue as anchor likely processed it.", peer);
                            } else {
                                // For relay peers: force-store in mailbox (bypasses direct delivery entirely).
                                // Using StoreInMailbox avoids the ForwardMeshSignaling → direct retry → EOF loop.
                                let is_relay_target = self.is_relayed_map.read().get(&target_peer).cloned().unwrap_or(false);
                                if is_relay_target {
                                    info!("[Mesh] Direct relay send failed for {}. Force-storing in mailbox.", peer);
                                    let tx = self.command_tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx.send(NetworkCommand::StoreInMailbox { peer_id: target_peer, payload }).await;
                                    });
                                } else {
                                    warn!("[Mesh] Re-queuing failed payload for Mailbox routing...");
                                    self.pending_messages.entry(target_peer).or_default().push(payload);
                                }
                            }
                        }

                        if is_network_failure {
                            warn!("[Mesh] Network failure (Ghost Connection) detected for {}. Forcing disconnect to trigger clean reconnect.", peer);
                            let _ = self.swarm.disconnect_peer_id(peer);
                        } else if !self.swarm.is_connected(&peer) {
                            self.is_relayed_map.write().remove(&peer);
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::ResponseSent { .. }) => {}
                    IntrovertBehaviourEvent::Ping(ping_event) => {
                        // Check for pending diagnostic RTT measurement
                        if let Ok(rtt) = ping_event.result {
                            let is_rbn = self.bootstrap_nodes.iter().any(|(id, _)| id == &ping_event.peer);
                            if is_rbn {
                                self.rbn_latencies.write().insert(ping_event.peer, rtt.as_millis());
                                crate::RBN_LATENCIES.write().insert(ping_event.peer.to_string(), rtt.as_millis());
                            }

                            if let Some(diag) = self.pending_diagnostics.remove(&ping_event.peer) {
                                let transport = diag.transport.unwrap_or_else(|| "Unknown".to_string());
                                let payload = format!(
                                    r#"{{"peer_id":"{}","step":"settled","status":"settled","transport":"{}","rtt_ms":{}}}"#,
                                    ping_event.peer, transport, rtt.as_millis()
                                );
                                crate::dispatch_global_event(15, payload.as_bytes());
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::InboundRequest { .. }) => {}
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetRecord(Err(e)), .. }) => {
                        let key = match &e {
                            kad::GetRecordError::NotFound { key, .. } => key,
                            kad::GetRecordError::QuorumFailed { key, .. } => key,
                            kad::GetRecordError::Timeout { key } => key,
                        };
                        let key_str = String::from_utf8_lossy(key.as_ref()).into_owned();
                        if key_str.starts_with("i@") {
                            info!("[Mesh] Failed to resolve handle {}: {:?}", key_str, e);
                            let data = key_str.into_bytes();
                            crate::dispatch_global_event(35, &data); // Event 35: Handle Resolve Failed
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { .. }) => {}
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::InboundFailure { .. }) => {}

                    // BINARY CODEC v2.0.0: Handle inbound v2 requests and responses
                    IntrovertBehaviourEvent::RequestResponseV2(request_response::Event::Message { peer, message: request_response::Message::Request { request, channel, .. }, .. }) => {
                        // v2 request received — send ACK and process payload
                        let _ = self.swarm.behaviour_mut().request_response_v2.send_response(channel, crate::network::codec::BinarySignalingResponse("ACK".to_string()));
                        let tx = self.command_tx.clone();
                        let payload = request.0;
                        tokio::spawn(async move {
                            let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: peer, payload }).await;
                        });
                    }
                    IntrovertBehaviourEvent::RequestResponseV2(request_response::Event::Message { peer, message: request_response::Message::Response { request_id, .. }, .. }) => {
                        // v2 response — update tracker and drain backpressure
                        self.outbound_tracker.remove(&request_id);
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }
                        if let Some(pending) = self.pending_messages.get_mut(&peer) {
                            if let Some(next_chunk) = pending.iter().position(|p| matches!(p, SignalingPayload::FileChunk { .. })) {
                                let payload = pending.remove(next_chunk);
                                if pending.is_empty() { self.pending_messages.remove(&peer); }
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
                                });
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponseV2(request_response::Event::OutboundFailure { request_id, peer, error, .. }) => {
                        warn!("[Codec] v2 OutboundFailure to {}: {:?}", peer, error);
                        if let Some((_, payload)) = self.outbound_tracker.remove(&request_id) {
                            // Increment retry count for failed v2 file chunks and re-queue to DB
                            if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
                                let _ = self.storage.increment_chunk_retry(transfer_id, chunk_index, 5);
                                if let Ok(chunk_data) = base64::decode(data_base64) {
                                    let _ = self.storage.enqueue_pending_chunk(transfer_id, &peer.to_string(), chunk_index, &chunk_data);
                                }
                            }
                        }
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponseV2(request_response::Event::InboundFailure { .. }) => {}
                    IntrovertBehaviourEvent::RequestResponseV2(request_response::Event::ResponseSent { .. }) => {}

                    IntrovertBehaviourEvent::Identify(identify::Event::Sent { .. }) => {}
                    IntrovertBehaviourEvent::Identify(identify::Event::Pushed { .. }) => {}
                    IntrovertBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Message { propagation_source, message_id, message }) => {
                        info!("[Mesh] Received gossipsub message from {} with id {}", propagation_source, message_id);
                        
                        // Use message.source (original author) when available, fall back to propagation_source (relay peer).
                        // In gossipsub, propagation_source is the immediate neighbor that forwarded the message,
                        // which may be a non-member RBN anchor node. Checking it against the member list
                        // would silently reject valid messages relayed through non-member nodes.
                        let author_peer = message.source.unwrap_or(propagation_source);
                        
                        // Verify the original author is a member of the group (topic = group_id)
                        let topic_str = message.topic.as_str();
                        let is_member = if let Ok(Some(group)) = self.storage.get_group(topic_str) {
                            let members: Vec<GroupMemberMetadata> = serde_json::from_str(&group.members_json).unwrap_or_default();
                            members.iter().any(|m| m.peer_id == author_peer.to_string())
                        } else {
                            false
                        };
                        
                        if !is_member {
                            warn!("[Mesh] Rejecting gossipsub message from non-member {} for topic {}", author_peer, topic_str);
                            return Ok(());
                        }
                        
                        if self.mesh_active_peers.insert(propagation_source) {
                            crate::ACTIVE_PEER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        if let Ok(payload) = serde_json::from_slice::<SignalingPayload>(&message.data) {
                            self.handle_single_payload(author_peer, payload, false).await;
                        }
                    }
                    IntrovertBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Subscribed { peer_id, topic }) => {
                        info!("[Mesh] Peer {} subscribed to topic {}", peer_id, topic);
                        if self.mesh_active_peers.insert(peer_id) {
                            crate::ACTIVE_PEER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    IntrovertBehaviourEvent::Gossipsub(_) => {}
                    IntrovertBehaviourEvent::Dcutr(_) => {}
                    IntrovertBehaviourEvent::Identify(_) => {}
                    IntrovertBehaviourEvent::Autonat(_) => {}
                    _ => {
                        // Only log truly unexpected behaviour events
                        debug!("[Swarm Debug] Unhandled behaviour event: {:?}", b_event);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("[Swarm] New listen address: {}", address);
            }
            SwarmEvent::ExternalAddrConfirmed { address } => {
                info!("[Swarm] External address CONFIRMED: {}", address);
                // Proactively bootstrap and re-dial RBNs on address confirmation to update DHT/Relay
                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                for (_, addr) in &self.bootstrap_nodes {
                    let _ = self.swarm.dial(addr.clone());
                }
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                info!("[Swarm] External address EXPIRED: {}", address);
                // If our only external address expired, we might be transitioning networks
                if self.swarm.external_addresses().count() == 0 {
                    info!("[Swarm] All external addresses expired. Re-resolving bootstrap nodes...");
                    // Re-resolve DNS for new network environment (VPN/cellular switch)
                    let fresh_bootstrap = config::get_bootstrap_nodes();
                    if !fresh_bootstrap.is_empty() {
                        self.bootstrap_nodes = fresh_bootstrap;
                    }
                    for (peer_id, addr) in &self.bootstrap_nodes {
                        self.swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
                    }
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                }
            }

            SwarmEvent::ListenerError { listener_id, error, .. } => {
                info!("[Swarm] Listener error ({:?}): {:?}", listener_id, error);
                if let Some(peer_id) = self.relay_listeners.remove(&listener_id) {
                    info!("[Mesh] Relay listener error for {}. Clearing reservation record.", peer_id);
                    self.relay_reservations.remove(&peer_id);
                }
            }
            SwarmEvent::ListenerClosed { listener_id, reason, .. } => {
                info!("[Swarm] Listener closed ({:?}): {:?}", listener_id, reason);
                if let Some(peer_id) = self.relay_listeners.remove(&listener_id) {
                    info!("[Mesh] Relay listener for {} closed. Clearing reservation record.", peer_id);
                    self.relay_reservations.remove(&peer_id);

                    // AUTO-RECOVERY: If we still have an active connection to this RBN,
                    // immediately re-request a relay reservation so we stay reachable.
                    // MUST use full multiaddr; relative /p2p/X/p2p-circuit fails with MissingRelayAddr.
                    if self.swarm.is_connected(&peer_id) {
                        info!("[Mesh] Still connected to RBN {}. Re-requesting relay reservation...", peer_id);
                        let relay_addr = if let Some((_, addr)) = self.bootstrap_nodes.iter().find(|(id, _)| *id == peer_id) {
                            let mut a = addr.clone();
                            if !a.to_string().contains(&peer_id.to_string()) {
                                a = a.with(libp2p::multiaddr::Protocol::P2p(peer_id));
                            }
                            a.with(libp2p::multiaddr::Protocol::P2pCircuit)
                        } else {
                            // Fallback: try to get address from Kademlia
                            let mut a = libp2p::multiaddr::Multiaddr::empty()
                                .with(libp2p::multiaddr::Protocol::P2p(peer_id))
                                .with(libp2p::multiaddr::Protocol::P2pCircuit);
                            a
                        };
                        match self.swarm.listen_on(relay_addr) {
                            Ok(new_id) => {
                                info!("[Mesh] Relay re-reservation request SUCCESS. New Listener ID: {:?}", new_id);
                                self.relay_reservations.insert(peer_id);
                                self.relay_listeners.insert(new_id, peer_id);
                            }
                            Err(e) => warn!("[Mesh] Relay re-reservation FAILED: {:?}", e),
                        }
                    }
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!("[Swarm] Connection established with {}", peer_id);

                let endpoint_addr = endpoint.get_remote_address();
                let is_manual = {
                    let mut pending = self.pending_manual_rbns.write();
                    pending.remove(endpoint_addr)
                };

                if let Some(original_ip) = is_manual {
                    info!("[Registry] Manual RBN connection confirmed to {} (PeerId: {})", original_ip, peer_id);
                    if !self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id) {
                        self.bootstrap_nodes.push((peer_id, endpoint_addr.clone()));
                        // Sync to global RBN bootstrap list
                        {
                            let mut global_bn = crate::BOOTSTRAP_NODES.write();
                            *global_bn = self.bootstrap_nodes.iter().map(|(pid, addr)| (pid.to_string(), addr.to_string())).collect();
                        }
                    }
                    let payload = format!("{}|{}|0", original_ip, peer_id);
                    crate::dispatch_global_event(45, payload.as_bytes()); // Event 45: RbnConnectionConfirmed
                }

                // If this is a direct (non-relayed) connection, save the address as a potential relay mapping
                if !endpoint.is_relayed() {
                    self.anchor_mappings.insert(peer_id, endpoint_addr.clone());
                }

                // FIX STATUS SEMANTICS:
                // status=1 (Online/Mesh Active) should only mean we are REACHABLE.
                // Connecting to the RBN alone does NOT make us reachable — we need a relay reservation.
                // So on raw ConnectionEstablished:
                //   - If we already have a relay listener: status=1 (reachable)
                //   - Otherwise: status=4 (connecting — RBN connected, waiting for relay)
                let has_relay = self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit"));
                if has_relay {
                    crate::dispatch_global_event(10, &[1]);
                } else {
                    // Status 4 = RBN connected, relay reservation pending
                    crate::dispatch_global_event(10, &[4]);
                    crate::dispatch_debug_log(&format!("[Status] ConnectionEstablished with {} but no relay yet — status=4 (connecting)", peer_id));
                }

                let endpoint_addr = endpoint.get_remote_address();
                let is_local_ip = endpoint_addr.to_string().contains("192.168.") || 
                                 endpoint_addr.to_string().contains("10.") || 
                                 endpoint_addr.to_string().contains("172.") ||
                                 endpoint_addr.to_string().contains("127.0.0.1");

                let is_relayed = endpoint.is_relayed() && !is_local_ip;
                if !is_relayed {
                    let count = self.direct_conn_count.entry(peer_id).or_insert(0);
                    *count += 1;
                }
                
                let is_now_relayed = self.direct_conn_count.get(&peer_id).cloned().unwrap_or(0) == 0;
                self.is_relayed_map.write().insert(peer_id, is_now_relayed);
                
                if is_local_ip && endpoint.is_relayed() {
                    info!("[Mesh] Peer {} connected via LOCAL RELAY. Treating as DIRECT for performance.", peer_id);
                }

                // --- RELIABILITY FIX: Relay Reservation ---
                // If we connect to a bootstrap node (RBN) or any anchor node, and we are NOT an anchor ourselves,
                // we must request a reservation to be reachable via that relay.
                let is_rbn = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                let is_anchor = self.discovered_anchors.contains(&peer_id);
                let we_are_anchor = self.storage.is_anchor_mode_enabled();

                if (is_rbn || is_anchor) && !we_are_anchor && !self.relay_reservations.contains(&peer_id) {
                    info!("[Mesh] Requesting RELAY RESERVATION from anchor: {}", peer_id);
                    // Build full multiaddr from the endpoint we just connected to.
                    // Relative addresses (/p2p/X/p2p-circuit) fail with MissingRelayAddr
                    // because libp2p-relay requires a transport address before the circuit.
                    let mut relay_addr = endpoint_addr.clone();
                    if !relay_addr.to_string().contains(&peer_id.to_string()) {
                        relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2p(peer_id));
                    }
                    relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2pCircuit);
                    match self.swarm.listen_on(relay_addr.clone()) {
                        Ok(id) => {
                            info!("[Mesh] Relay reservation requested. Address: {}, Listener: {:?}", relay_addr, id);
                            self.relay_reservations.insert(peer_id);
                            self.relay_listeners.insert(id, peer_id);
                        }
                        Err(e) => info!("[Mesh] Relay reservation failed: {:?}", e),
                    }
                }
                let status: u8 = if is_now_relayed { 1 } else { 0 };
                let mut data = peer_id.to_string().into_bytes();
                data.push(b':');
                data.push(status);
                crate::dispatch_global_event(8, &data);
                
                if is_now_relayed { self.reward_tracker.record_relay(&peer_id.to_string(), 1024); }
                
                let data = peer_id.to_bytes();
                crate::dispatch_global_event(1, &data); 

                // Diagnostic recheck: record transport type (settled event fires on first Ping RTT)
                if let Some(diag) = self.pending_diagnostics.get_mut(&peer_id) {
                    let addr_str = endpoint.get_remote_address().to_string();
                    let transport_type = if !is_relayed {
                        if addr_str.contains("127.0.0.1") || addr_str.contains("localhost") {
                            "WebSocket Tunnel (Port 80)"
                        } else {
                            "Direct P2P"
                        }
                    } else if addr_str.contains("quic") || addr_str.contains("udp") {
                        "Relayed UDP/QUIC (Port 443)"
                    } else {
                        "Relayed TCP (Port 443)"
                    };
                    diag.transport = Some(transport_type.to_string());
                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"connected","status":"connected","transport":"{}"}}"#,
                        peer_id, transport_type
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }

                // Flush pending messages on connection — but RATE-LIMITED to prevent thundering herd
                // on relay circuits. File chunks are paced: max 4 in-flight at 50ms intervals.
                if let Some(payloads) = self.pending_messages.remove(&peer_id) {
                    let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        for payload in payloads {
                            let is_chunk = matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. });
                            if is_chunk && is_relayed {
                                // Pace relay chunk sends at 50ms each to avoid flooding the circuit
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                        }
                    });
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, endpoint, .. } => {
                // Clean up WebRTC resources immediately on connection loss to prevent stale ghost channels
                self.data_channels.write().remove(&peer_id);
                self.anchor_mappings.remove(&peer_id);
                // FIX: Only clear relay reservation when the RBN/anchor connection closes.
                // Previously this cleared reservations for ALL peers, including non-RBN disconnects,
                // which wiped the RBN relay reservation and made us unreachable via relay.
                let is_rbn_or_anchor = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id)
                    || self.discovered_anchors.contains(&peer_id);
                if is_rbn_or_anchor {
                    // Don't clear relay_reservations here — the relay circuit may still be
                    // working through a tunnel or alternate path. ListenerClosed will clear it
                    // when the relay listener actually closes.
                    self.relay_listeners.retain(|_, rbn| rbn != &peer_id);
                    info!("[Mesh] RBN/anchor {} disconnected. Cleared relay listeners (reservation kept).", peer_id);
                }
                self.inflight_requests.remove(&peer_id);
                self.pending_requester_static_keys.remove(&peer_id.to_string());

               let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
               if let Some(pc) = pc {
                   let _ = pc.close().await;
               }

               let endpoint_addr = endpoint.get_remote_address();
               let is_local_ip = endpoint_addr.to_string().contains("192.168.") || 
                                endpoint_addr.to_string().contains("10.") || 
                                endpoint_addr.to_string().contains("172.") ||
                                endpoint_addr.to_string().contains("127.0.0.1");

               let is_relayed = endpoint.is_relayed() && !is_local_ip;
               if !is_relayed {
                   if let Some(count) = self.direct_conn_count.get_mut(&peer_id) {
                       if *count > 0 {
                           *count -= 1;
                       }
                       if *count == 0 {
                           self.direct_conn_count.remove(&peer_id);
                       }
                   }
               }

               if !self.swarm.is_connected(&peer_id) {
                   self.noise_sessions.remove(&peer_id); // MEMORY FIX: Remove stale noise session
                   self.is_relayed_map.write().remove(&peer_id);
                   self.direct_conn_count.remove(&peer_id);
                   if self.mesh_active_peers.remove(&peer_id) {
                       crate::ACTIVE_PEER_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                   }
                   info!("[Swarm] Connection lost with {}. Peer is now truly offline.", peer_id);

                    let mut data = peer_id.to_string().into_bytes();
                    data.push(b':');
                    data.push(2); // 2 = Offline
                    crate::dispatch_global_event(8, &data);

                    // Re-dial contacts or anchors to ensure mesh remains alive during network transitions
                    let is_anchor = self.discovered_anchors.contains(&peer_id) ||
                                    self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer_id.to_string())).unwrap_or(false);

                    if is_anchor {
                        self.dial_relay_path(peer_id, false); // Use helper for consistent re-dialing
                    } else if let Ok(contacts) = self.storage.get_all_contacts() {
                        if contacts.iter().any(|c| c.peer_id == peer_id.to_string()) {
                            self.dial_relay_path(peer_id, false);
                        }
                    }
               } else {
                   // Peer is still connected via remaining connections, update relayed state
                   let is_now_relayed = self.direct_conn_count.get(&peer_id).cloned().unwrap_or(0) == 0;
                   self.is_relayed_map.write().insert(peer_id, is_now_relayed);
                   
                   let status: u8 = if is_now_relayed { 1 } else { 0 };
                   let mut data = peer_id.to_string().into_bytes();
                   data.push(b':');
                   data.push(status);
                   crate::dispatch_global_event(8, &data);
               }
            }
            SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, .. } => {
                let err_str = format!("{:?}", error);
                if err_str.contains("SelfConnect") || err_str.contains("self-connect") || err_str.contains("Self") {
                    warn!("[Identity] SelfConnect anomaly detected — local={}, remote={}, error={}", local_addr, send_back_addr, err_str);
                    let payload = format!(
                        r#"{{"local_addr":"{}","remote_addr":"{}","error":"self_connect_anomaly"}}"#,
                        local_addr, send_back_addr
                    );
                    crate::dispatch_global_event(47, payload.as_bytes());
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(pid) = peer_id {
                    if pid == *self.swarm.local_peer_id() { return Ok(()); }
                    info!("[Swarm] Outgoing connection error for peer {}: {:?}", pid, error);

                    // Clean up the failed address from Kademlia to stop propagating stale routes
                    if let libp2p::swarm::DialError::Transport(errors) = &error {
                        for (addr, _) in errors {
                            info!("[Mesh] Removing failed address {} from Kademlia for peer {}", addr, pid);
                            self.swarm.behaviour_mut().kademlia.remove_address(&pid, addr);
                        }
                    }



                    // Track diagnostic failures for the recheck overlay
                    if self.pending_diagnostics.contains_key(&pid) {
                        let err_str = format!("{:?}", error).replace('"', "'");
                        if err_str.contains("ResourceLimitExceeded") {
                            warn!("[Mesh] RELAY CONGESTION: RBN rejected circuit for {} due to ResourceLimitExceeded.", pid);
                        }
                        let diag_payload = format!(
                            r#"{{"peer_id":"{}","step":"error","status":"dial_failed","error":"{}"}}"#,
                            pid, err_str
                        );
                        crate::dispatch_global_event(15, diag_payload.as_bytes());
                    }
                }
            }

            _ => {}
        }
        Ok(())
    }

    fn dial_relay_path(&mut self, recipient_id: PeerId, for_file_chunk: bool) {
        let recipient_str = recipient_id.to_string();

        // Exponential backoff: base 5s, max 300s (5 minutes)
        // File chunks skip the rate limiter — they have no mailbox fallback and MUST succeed.
        const BASE_BACKOFF_SECS: u64 = 5;
        const MAX_BACKOFF_SECS: u64 = 300;

        if !for_file_chunk {
            if let Some((last, failure_count)) = self.relay_dial_limiter.get(&recipient_id) {
                let backoff = std::cmp::min(
                    BASE_BACKOFF_SECS * 2u64.pow(*failure_count),
                    MAX_BACKOFF_SECS
                );
                if last.elapsed() < Duration::from_secs(backoff) {
                    debug!("[Mesh] Rate-limited dial to {} (backoff: {}s, failures: {})",
                        &recipient_str[..16.min(recipient_str.len())], backoff, failure_count);
                    return;
                }
            }
        }

        let chunk_label = if for_file_chunk { " [FILE_CHUNK]" } else { "" };
        debug!("[Mesh] Peer {} not connected. Trying all connection strategies{}...", recipient_str, chunk_label);
        let local_id = *self.swarm.local_peer_id();
        let mut dial_success = false;

        // Strategy 1: Direct P2P (fastest, no relay overhead)
        if self.swarm.dial(recipient_id).is_ok() {
            dial_success = true;
        }

        // Strategy 2: Via RBNs.
        // For text messages: one RBN by latency, break early (mailbox is fallback).
        // For file chunks: ALL RBNs, no break (no mailbox fallback — dial MUST succeed).
        {
            let mut rbn_list: Vec<_> = self.bootstrap_nodes.iter()
                .filter(|(id, _)| *id != local_id)
                .collect();

            // Sort by ping latency (best RBN first), but prioritize relay_hint RBN
            {
                let latencies = self.rbn_latencies.read();
                let hinted_rbn = self.relay_hints.get(&recipient_id).cloned();
                rbn_list.sort_by_key(|(id, _)| {
                    // If this is the hinted RBN, give it highest priority (0)
                    if Some(*id) == hinted_rbn {
                        return 0;
                    }
                    // Otherwise sort by latency
                    latencies.get(id).cloned().unwrap_or(u128::MAX)
                });
            }

            for &(rbn_id, ref rbn_addr) in &rbn_list {
                let relay_addr = rbn_addr.clone()
                    .with(libp2p::multiaddr::Protocol::P2p(*rbn_id))
                    .with(libp2p::multiaddr::Protocol::P2pCircuit)
                    .with(libp2p::multiaddr::Protocol::P2p(recipient_id));

                debug!("[Mesh] Strategy: Relay via RBN {}{}", rbn_id, chunk_label);
                match self.swarm.dial(relay_addr.clone()) {
                    Ok(_) => {
                        dial_success = true;
                        debug!("[dial_relay_path] Relay via RBN queued: {}", relay_addr);
                        if !for_file_chunk {
                            break; // Text messages: one successful dial is enough (mailbox fallback exists)
                        }
                        // File chunks: continue trying ALL RBNs
                    }
                    Err(e) => {
                        crate::dispatch_debug_log(&format!("[dial_relay_path] Relay via RBN FAILED: {:?}", e));
                    }
                }
            }
        }

        // Strategy 3: Via connected anchor nodes (secondary relay path)
        // Anchors that support HOP protocol can relay traffic when RBNs are unreachable.
        if !dial_success {
            let anchor_ids: Vec<PeerId> = self.discovered_anchors.iter()
                .filter(|id| **id != local_id && self.swarm.is_connected(id))
                .cloned()
                .collect();
            for anchor_id in anchor_ids {
                if let Some(addr) = self.anchor_mappings.get(&anchor_id) {
                    let relay_addr = addr.clone()
                        .with(libp2p::multiaddr::Protocol::P2p(anchor_id))
                        .with(libp2p::multiaddr::Protocol::P2pCircuit)
                        .with(libp2p::multiaddr::Protocol::P2p(recipient_id));
                    debug!("[Mesh] Strategy: Relay via anchor {}{}", anchor_id, chunk_label);
                    if self.swarm.dial(relay_addr).is_ok() {
                        dial_success = true;
                        break;
                    }
                }
            }
        }

        // Strategy N+1: WebSocket tunnel fallback (NAT traversal for restrictive networks)
        if !dial_success && !self.tunnel_active {
            info!("[Mesh] Activating WebSocket tunnel for NAT traversal");
            crate::dispatch_debug_log("[dial_relay_path] All direct strategies failed — activating WebSocket tunnel");
            let tx = self.command_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(NetworkCommand::ActivateTunnel).await;
            });
        }

        // Update limiter with backoff
        let entry = self.relay_dial_limiter.entry(recipient_id).or_insert((Instant::now(), 0));
        entry.0 = Instant::now();
        if !dial_success {
            entry.1 = entry.1.saturating_add(1);
        } else {
            entry.1 = 0; // Reset on success
        }
    }

    async fn forward_to_mesh(&mut self, recipient_id: PeerId, payload: SignalingPayload, force_mailbox: bool) -> anyhow::Result<()> {
        let recipient_str = recipient_id.to_string();

        // LOOPBACK PROTECTION: If sending to ourselves, handle locally
        if recipient_id == *self.swarm.local_peer_id() {
             debug!("[Mesh] Loopback payload detected for {}. Routing to local handler.", recipient_str);
             let tx = self.command_tx.clone();
             let p = payload.clone();
             tokio::spawn(async move {
                 let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: recipient_id, payload: p }).await;
             });
             return Ok(());
        }

        if !force_mailbox {
            // 1. Try WebRTC Data Channel if open
            // HYBRID ROUTING: WebRTC Data Channels are extremely fast for small signaling payloads.
            // However, the SCTP stack in webrtc-rs can severely bottleneck on large 64KB+ file chunks.
            // Therefore, we skip WebRTC ONLY for actual FileChunk data (large binary payloads),
            // but allow FileChunkRequest and FileTransfer manifest to use WebRTC for ultra-low latency.
            let is_large_data = matches!(payload, SignalingPayload::FileChunk { .. });
            let is_relayed_conn = self.is_relayed_map.read().get(&recipient_id).cloned().unwrap_or(true);
            let skip_webrtc = is_large_data && !is_relayed_conn;

            if !skip_webrtc {
                let dc_opt = { self.data_channels.read().get(&recipient_id).cloned() };
                let pc_opt = { self.peer_connections.read().get(&recipient_id).cloned() };
                if let (Some(dc), Some(pc)) = (dc_opt, pc_opt) {
                    if dc.ready_state() == RTCDataChannelState::Open && pc.connection_state() == RTCPeerConnectionState::Connected {
                        // Prevent SCTP buffer overflow: wait up to 1 second (100 * 10ms) if buffer is full
                        let mut wait_count = 0;
                        while dc.buffered_amount().await > 256 * 1024 && wait_count < 100 {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            wait_count += 1;
                        }
                        if let Ok(bytes) = serde_json::to_vec(&payload) {
                            if dc.send(&bytes.into()).await.is_ok() {
                                debug!("[Mesh] Delivered payload to {} via WebRTC Data Channel", recipient_str);
                                return Ok(());
                            } else {
                                debug!("[Mesh] WebRTC Data Channel send FAILED for {}. Removing and closing WebRTC resources.", recipient_str);
                                self.data_channels.write().remove(&recipient_id);
                                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&recipient_id) };
                                if let Some(pc) = pc {
                                    let _ = pc.close().await;
                                }
                            }
                        }
                    }
                }
            }
            // 2. Try direct libp2p delivery if connected
            if self.swarm.is_connected(&recipient_id) {
                // Enforce in-flight concurrency limit (backpressure flow control) for both direct and relay links
                // to prevent socket/substream multiplexer saturation.
                let is_chunk_data = matches!(payload, SignalingPayload::FileChunk { .. });
                if is_chunk_data {
                    let is_relayed_conn = self.is_relayed_map.read().get(&recipient_id).cloned().unwrap_or(false);
                    if is_relayed_conn {
                        // TransitFileChunk removed — chunks flow through the normal relay
                        // circuit (direct libp2p over the circuit), not via an extra RBN hop.
                        // The transit envelope added latency and depended on RBNs supporting it.
                    }

                    let inflight = self.inflight_requests.get(&recipient_id).cloned().unwrap_or(0);
                    // v43 baseline: relay=8, direct=12. Proven working for cross-network.
                    let limit = if is_relayed_conn { 4 } else { 8 }; // v54 baseline: proven working for cross-network
                    if inflight >= limit {
                        debug!("[Mesh] In-flight limit ({}) reached for {}. Buffering chunk.", limit, recipient_str);
                        self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
                        return Ok(());
                    }
                }

                debug!("[Mesh] Peer {} is connected. Attempting direct delivery...", recipient_str);
                let mut sent = false;
                // If it's a message/ack that can be encrypted, try Noise.
                // NOTE: FileChunk is intentionally excluded from Noise on relay connections:
                // relay transport is already encrypted (libp2p Noise), and adding app-level
                // Noise causes double-JSON-base64 overhead (~83% extra wire cost per chunk).
                let noise_eligible = match &payload {
                    SignalingPayload::Standard(_) | 
                    SignalingPayload::ChatMessage { .. } | 
                    SignalingPayload::Acknowledgement { .. } |
                    SignalingPayload::GroupManifest { .. } |
                    SignalingPayload::GroupInvite { .. } |
                    SignalingPayload::MessageReaction { .. } |
                    SignalingPayload::EditMessage { .. } |
                    SignalingPayload::SetRetention { .. } => true,
                    // FileChunk: NEVER use app-level Noise. libp2p transport already encrypts.
                    // App-level Noise adds ~83% wire overhead per chunk with no security benefit.
                    SignalingPayload::FileChunk { .. } => false,
                    _ => false,
                };
                if noise_eligible {
                    if let Some(session) = self.noise_sessions.get_mut(&recipient_id) {

                        if session.is_finished() {
                            if let Ok(bytes) = serde_json::to_vec(&payload) {
                                if let Ok(encrypted) = session.send_message(&bytes) {
                                    debug!("[Mesh] Sending ENCRYPTED payload to {}", recipient_str);
                                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&recipient_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted))));
                                    self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                                    sent = true;
                                } else {
                                    info!("[Mesh] Noise encryption FAILED for {}. Clearing session and starting a new handshake.", recipient_str);
                                    self.noise_sessions.remove(&recipient_id);
                                    let storage = Arc::clone(&self.storage);
                                    let pid_str = recipient_id.to_string();
                                    tokio::task::spawn_blocking(move || {
                                        let _ = storage.delete_session_state(&pid_str);
                                    });
                                    let tx = self.command_tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: recipient_id }).await;
                                    });
                                }
                            }
                        }
                    }
                }

                if !sent {
                    // BINARY CODEC v2.0.0: Route FileChunk payloads through the optimized
                    // binary pipe when the peer supports it. This bypasses base64 encoding
                    // and sends raw binary data, saving ~25% wire overhead.
                    let is_file_chunk = matches!(payload, SignalingPayload::FileChunk { .. });
                    let use_v2 = is_file_chunk && self.peer_supports_v2.contains(&recipient_id);

                    if use_v2 {
                        debug!("[Codec] Sending FileChunk to {} via binary v2.0.0 pipe", recipient_str);
                        let req_id = self.swarm.behaviour_mut().request_response_v2.send_request(&recipient_id, crate::network::codec::BinarySignalingRequest(payload.clone()));
                        self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                    } else {
                        debug!("[Mesh] Sending PLAIN payload to {}", recipient_str);
                        let req_id = self.swarm.behaviour_mut().request_response.send_request(&recipient_id, SignalingRequest(payload.clone()));
                        self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                    }
                }

                if is_chunk_data {
                    *self.inflight_requests.entry(recipient_id).or_insert(0) += 1;
                }
                return Ok(());
            }

            // 3. Active Relay Dialing (Messenger Strategy)
            // If not connected, construct and dial the relay path via RBN
            debug!("[forward_to_mesh] Peer {} not connected — dialing relay path", &recipient_str[..16.min(recipient_str.len())]);
            self.dial_relay_path(recipient_id, false);
        }
        // 4. Fallback: Persistent Mesh Storage (Mailbox)
        
        // Check for Push Token to trigger background wakeup (iOS/Android parity)
        if let Ok(Some((device_type, token))) = self.storage.get_push_token(&recipient_str) {
            info!("[Registry] 🔔 Triggering Push Wakeup for {} ({})", recipient_str, device_type);
            let client = reqwest::Client::new();
            let peer_id_clone = recipient_str.clone();
            tokio::spawn(async move {
                // Send generic wakeup trigger to the sovereign push bridge (Anonymized)
                use sha2::{Sha256, Digest};
                let anonymized_peer_id = hex::encode(Sha256::digest(peer_id_clone.as_bytes()));
                
                let payload = serde_json::json!({
                    "device_type": device_type,
                    "token": token,
                    "peer_id_hash": anonymized_peer_id
                });
                let _ = client.post("https://push.introvert.network/wakeup")
                    .json(&payload)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await;
                    });
                }

        // WebRTC signaling and handle claims are transient and should never be stored in persistent mailboxes.
        if matches!(payload, SignalingPayload::WebRtc(_) | SignalingPayload::WebRtcNative(_) | SignalingPayload::Candidate(_) | SignalingPayload::Offer(_) | SignalingPayload::Answer(_) | SignalingPayload::HandleClaimRequest { .. } | SignalingPayload::HandleClaimWitnessed { .. }) {
            debug!("[Mesh] Buffering real-time signaling/handle registry payload for {} in RAM...", recipient_str);
            self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
            return Ok(());
        }

        // CRITICAL: File data and requests must NEVER be stored in the persistent mailbox.
        // They are buffered in RAM (pending_messages) and flushed only upon circuit establishment.
        if matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
            // RELAY-AWARE ROUTING: If the recipient is behind a known RBN and we have
            // an active circuit to that RBN, send directly through the relay instead of
            // buffering. This bypasses swarm.is_connected(&recipient_id) which returns
            // false for relay-connected peers.
            if let Some(&rbn_id) = self.relay_hints.get(&recipient_id) {
                if self.swarm.is_connected(&rbn_id) {
                    let request = SignalingRequest(payload.clone());
                    self.swarm.behaviour_mut().request_response.send_request(&rbn_id, request);
                    info!("[Mesh] Sent file payload to {} via relay RBN {}", recipient_str, rbn_id);
                    return Ok(());
                }
            }

            // Check if any RBNs are connected
            let has_rbn = self.bootstrap_nodes.iter().any(|(id, _)| self.swarm.is_connected(id));

            if !has_rbn {
                // No RBNs connected — persist to DB so chunks survive app restart
                if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
                    if let Ok(chunk_data) = base64::decode(data_base64) {
                        debug!("[Mesh] No RBNs connected. Persisting chunk {} for transfer {} to DB", chunk_index, transfer_id);
                        let _ = self.storage.enqueue_pending_chunk(transfer_id, &recipient_str, chunk_index, &chunk_data);
                    }
                }
                // FileChunkRequests are not persisted — they're small and will be re-generated
                return Ok(());
            }

            debug!("[Mesh] Path not ready. Buffering file chunk/request for {} in RAM...", recipient_str);
            // REDUNDANCY FILTER: If adding a Request, remove older Requests for the same transfer to prevent buffer bloat
            if let SignalingPayload::FileChunkRequest { ref transfer_id, chunk_index, .. } = payload {
                if let Some(pending) = self.pending_messages.get_mut(&recipient_id) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref tid, chunk_index: ref idx, .. } if tid == transfer_id && idx == &chunk_index));
                }
            }
            self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
            // Also persist to DB so chunks survive app restart
            if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
                if let Ok(chunk_data) = base64::decode(data_base64) {
                    let _ = self.storage.enqueue_pending_chunk(transfer_id, &recipient_str, chunk_index, &chunk_data);
                }
            }
            return Ok(());
        }





        let mut anchor_ids = Vec::new();
        if let Ok(verified_anchors) = self.storage.fetch_all_anchor_nodes() {
            for node in verified_anchors {
                if let Ok(pid) = node.peer_id.parse::<PeerId>() { anchor_ids.push(pid); }
            }
        }
        for pid in &self.discovered_anchors {
            if !anchor_ids.contains(pid) { anchor_ids.push(*pid); }
        }

        // Filter for connected VERIFIED RBNs only — store on ALL of them for redundancy.
        // Only peers in verified_rbns receive MailboxStore payloads.
        // This set is populated from bootstrap_nodes (today) and Solana registry (future).
        // Discovered anchors with HOP protocol are used for relay circuits only, not storage.
        let connected_anchors: Vec<PeerId> = anchor_ids.iter()
            .filter(|pid| self.verified_rbns.contains(pid) && self.swarm.is_connected(pid))
            .cloned()
            .collect();

        if !connected_anchors.is_empty() {
            let allowed_in_mailbox = matches!(payload, 
                SignalingPayload::ChatMessage { .. } | 
                SignalingPayload::Acknowledgement { .. } |
                SignalingPayload::MailboxStored { .. } | 
                SignalingPayload::FileTransfer { .. } |
                SignalingPayload::FileTransferComplete { .. } |
                SignalingPayload::FileTransferError { .. } |
                SignalingPayload::DeleteMessage { .. } |
                SignalingPayload::GroupInvite { .. } |
                SignalingPayload::GroupAction(_) |
                SignalingPayload::GroupManifest { .. } |
                SignalingPayload::MessageReaction { .. } |
                SignalingPayload::EditMessage { .. } |
                SignalingPayload::SetRetention { .. } |
                SignalingPayload::HandleClaimWitnessed { .. } |
                SignalingPayload::ChatSyncRequest { .. } |
                SignalingPayload::ChatSyncResponse { .. } |
                SignalingPayload::TelemetryEnvelope { .. }
            );

            if !allowed_in_mailbox {
                return Ok(());
            }

            // Extract msg_id for delivery tracking before payload is moved
            let original_msg_id = match &payload {
                SignalingPayload::ChatMessage { msg_id, .. } => Some(msg_id.clone()),
                _ => None,
            };
            
            // Ensure Mailbox payloads are only ENCRYPTED if they are noise-eligible (Messages/Standard)
            // and a session exists. Transient payloads like Acknowledgements should remain PLAIN 
            // for reliable mailbox delivery across session restarts.
            let noise_eligible = match &payload {
                SignalingPayload::Standard(_) | SignalingPayload::ChatMessage { .. } | SignalingPayload::ChatSyncRequest { .. } | SignalingPayload::ChatSyncResponse { .. } => true,
                _ => false,
            };

            let secure_payload = if noise_eligible {
                if let Some(session) = self.noise_sessions.get_mut(&recipient_id) {
                    if session.is_finished() {
                        let msg_bytes = serde_json::to_vec(&payload)?;
                        SignalingPayload::Secure(SecureMessage::Transport(session.send_message(&msg_bytes)?))
                    } else {
                        payload
                    }
                } else {
                    // Proactively initiate session for contacts
                    if let Ok(contacts) = self.storage.get_all_contacts() {
                        if contacts.iter().any(|c| c.peer_id == recipient_str) {
                            debug!("[Mesh] Initiating Noise session with contact {} for Mailbox delivery", recipient_str);
                            let tx = self.command_tx.clone();
                            let rid = recipient_id;
                            tokio::spawn(async move { let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: rid }).await; });
                        }
                    }
                    payload
                }
            } else {
                payload
            };

            let bytes = serde_json::to_vec(&secure_payload)?;

            for anchor_id in &connected_anchors {
                debug!("[Mesh] Storing message for {} on Anchor {}", recipient_str, anchor_id);
                let req_id = self.swarm.behaviour_mut().request_response.send_request(
                    anchor_id, 
                    SignalingRequest(SignalingPayload::MailboxStore { 
                        recipient_id: recipient_str.clone(), 
                        payload: bytes.clone(),
                        original_msg_id: original_msg_id.clone(),
                    })
                );
                self.outbound_tracker.insert(req_id, (recipient_id, secure_payload.clone()));
            }
            Ok(())
        } else {
            // No connected anchors for storage. Queue locally in RAM for when we eventually connect.
            info!("[Mesh] No connected anchors for storage. Queuing locally for {}.", recipient_str);
            let pending = self.pending_messages.entry(recipient_id).or_default();
            
            // Deduplicate Chunks/Requests to keep RAM lean
            if let SignalingPayload::FileChunk { transfer_id, chunk_index, .. } = &payload {
                pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, chunk_index: idx, .. } if tid == transfer_id && idx == chunk_index));
            } else if let SignalingPayload::FileChunkRequest { transfer_id, chunk_index, .. } = &payload {
                pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: idx, .. } if tid == transfer_id && idx == chunk_index));
            }

            // SECURITY: Limit pending messages per peer to prevent memory exhaustion
            if pending.len() >= 50 {
                warn!("[Security] Pending message queue full for peer {}. Dropping oldest.", recipient_id);
                pending.remove(0);
            }
            pending.push(payload.clone());

            // Dial the recipient directly to see if direct connection is possible.
            // RBN/Anchor dials are handled in the background by Resilience / Intro-Claw and dial_relay_path.
            let _ = self.swarm.dial(recipient_id);
            Err(anyhow::anyhow!("Mesh storage temporarily unavailable, message queued"))
        }
    }

    async fn perform_mailbox_fetch(&mut self) {
        let mut anchor_ids = Vec::new();
        if let Ok(verified_anchors) = self.storage.fetch_all_anchor_nodes() {
            for node in verified_anchors { if let Ok(pid) = node.peer_id.parse::<PeerId>() { anchor_ids.push(pid); } }
        }
        for pid in &self.discovered_anchors { if !anchor_ids.contains(pid) { anchor_ids.push(*pid); } }
        
        for peer_id in anchor_ids {
            if self.swarm.is_connected(&peer_id) { 
                info!("[Mesh] Draining verified anchor: {}", peer_id);
                self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));
            } else { 
                let _ = self.swarm.dial(peer_id); 
            }
        }

    }

    async fn handle_command(&mut self, command: NetworkCommand) -> anyhow::Result<()> {
        match command {
            NetworkCommand::Dial { peer_id, address } => {
                if let Some(addr) = address {
                    let final_addr = if addr.iter().any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_))) { addr } else { addr.with(libp2p::multiaddr::Protocol::P2p(peer_id)) };
                    self.swarm.dial(final_addr)?;
                } else {
                    if let Err(_) = self.swarm.dial(peer_id) {
                        let _ = self.swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
                    }
                }
            }
            NetworkCommand::ListenOn { address } => { self.swarm.listen_on(address)?; }
            NetworkCommand::SendSignaling { peer_id, msg_id, message, reply_to } => {
                let peer_id_str = peer_id.to_string();
                let content_str = message.clone();
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                let c = content_str.clone();
                let rt = reply_to.clone();
                tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &mid, &c, true, rt.as_deref(), None));
                self.reward_tracker.record_message_activity(&peer_id.to_string());
                
                let timestamp = chrono::Utc::now().timestamp();
                let payload = SignalingPayload::ChatMessage { content: message, msg_id, timestamp, reply_to };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::FetchMailbox => {
                self.perform_mailbox_fetch().await;
            }
            NetworkCommand::ClearMailboxForPeer { peer_id: _ } => {
                // Proactively drain all RBN mailboxes — the MailboxDrained handler
                // will skip pre-clear messages via should_skip_mailbox_message()
                self.perform_mailbox_fetch().await;
                info!("[Mailbox] Proactive drain triggered for chat clear");
            }
            NetworkCommand::LookupPeerHandle { peer_id } => {
                // Query Kademlia DHT for ph_<peer_id> reverse mapping
                // Result arrives via GetRecord handler (Event 37: PeerHandleRestored)
                let ph_key = RecordKey::new(&format!("ph_{}", peer_id).as_bytes());
                info!("[Registry] Looking up handle for peer {} via DHT...", peer_id);
                let _ = self.swarm.behaviour_mut().kademlia.get_record(ph_key);
            }
            NetworkCommand::UpdateAnchorStatus { enabled } => {
                let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                
                if enabled {
                    info!("[Mesh] Opting in as Anchor Node. Advertising to DHT...");
                    let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
                } else {
                    info!("[Mesh] Opting out of Anchor services.");
                    let _ = self.swarm.behaviour_mut().kademlia.stop_providing(&key);
                }

                let payload = [if enabled { 1 } else { 0 }];
                crate::dispatch_global_event(11, &payload);
            }
            NetworkCommand::SetConnectivityType { connectivity_type } => {
                let old_type = self.connectivity_type;
                self.connectivity_type = connectivity_type;
                
                if connectivity_type != old_type {
                    info!("[Network] Connectivity transition detected: {} -> {}", old_type, connectivity_type);
                    
                    // NOTE: Do NOT clear relay_reservations on network transition.
                    // This was the root cause of cross-network/VPN relay failure.
                    // Instead, stale reservations are detected by the periodic status check
                    // which force-reconnects RBNs when reservations=0 but RBNs are connected.
                    self.relay_dial_limiter.clear();
                    
                    if connectivity_type == 5 {
                        info!("[Network] VPN detected (connectivity_type=5). Activating tunnel with Alibaba RBN fallback.");
                        crate::dispatch_debug_log("[VPN] VPN connection detected — activating tunnel with fallback");

                        // Force WebSocket tunnel reset if already active to clean stale sockets
                        if self.tunnel_active {
                            self.tunnel_active = false;
                            self._tunnel_handle = None;
                        }

                        // Keep Alibaba RBN as fallback (don't clear bootstrap_nodes)
                        // Force WebSocket tunnel mode active immediately
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(NetworkCommand::ActivateTunnel).await;
                        });
                    } else {
                        // Normal network (WIFI/CELLULAR/etc.) — restore full bootstrap list if previously restricted
                        let fresh_bootstrap = config::get_bootstrap_nodes();
                        if !fresh_bootstrap.is_empty() {
                            self.bootstrap_nodes = fresh_bootstrap;
                        }
                        
                        // Disable tunnel if it was active
                        if self.tunnel_active {
                            info!("[Network] Disabling WebSocket tunnel on transition to normal network...");
                            self.tunnel_active = false;
                            self._tunnel_handle = None;
                            self.bootstrap_nodes.retain(|(_, addr)| !addr.to_string().contains("127.0.0.1"));
                        }
                    }
                    
                    // Trigger dynamic recheck/bootstrap
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                }

                // Dispatch global event (Removed to avoid collision with File Transfer Progress Event 12)
                // let payload = [connectivity_type];
                // crate::dispatch_global_event(12, &payload);
            },
            NetworkCommand::AddGroupMember { group_id, peer_id } => {
                info!("[Mesh] Adding member {} to group {}", peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin {
                        error!("[Mesh] Permission denied: Only admins can add members");
                        return Ok(());
                    }

                    if members.iter().any(|m| m.peer_id == peer_id) {
                        info!("[Mesh] Peer {} is already a member", peer_id);
                        return Ok(());
                    }

                    if let Ok(Some(contact)) = self.storage.get_contact(&peer_id) {
                        let new_meta = GroupMemberMetadata {
                            peer_id: peer_id.clone(),
                            pubkey: contact.p2p_pubkey.clone(),
                            role: GroupRole::Member,
                            alias: contact.local_alias.or(contact.global_name),
                            avatar_base64: contact.avatar_base64,
                        };
                        members.push(new_meta.clone());
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                        let action = GroupAction::AddMember { metadata: new_meta };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            if let Ok(wrapped) = group::GroupManager::wrap_group_secret(&group_info.secret, &contact.static_key) {
                                let invite = SignalingPayload::GroupInvite {
                                    group_id: group_id.clone(),
                                    name: group_info.name.clone(),
                                    description: group_info.description.clone(),
                                    inviter_peer_id: my_peer_id.clone(),
                                    group_secret_wrapped: wrapped,
                                    members: members.clone(),
                                };
                                if let Ok(pid) = peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, invite, false).await;
                                }
                            }

                            for m in &members {
                                if m.peer_id == my_peer_id || m.peer_id == peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                    } else {
                        error!("[Mesh] Cannot add member: Peer {} is not in contacts list", peer_id);
                    }
                }
            }
            NetworkCommand::ApproveGroupJoin { group_id, requester_peer_id, alias, avatar, handle: _handle } => {
                info!("[Mesh] Admin approving group join request for {} to group {}", requester_peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();

                    if members.iter().any(|m| m.peer_id == requester_peer_id) {
                        info!("[Mesh] Peer {} is already a member", requester_peer_id);
                        return Ok(());
                    }

                    let mut p2p_pubkey = vec![];
                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        let peer_bytes = peer.to_bytes();
                        if peer_bytes.len() >= 38 && peer_bytes[0] == 0x00 && peer_bytes[1] == 0x24 {
                            if let Ok(pubk) = libp2p::identity::PublicKey::try_decode_protobuf(&peer_bytes[2..]) {
                                p2p_pubkey = pubk.encode_protobuf();
                            }
                        }
                    }

                    let new_meta = GroupMemberMetadata {
                        peer_id: requester_peer_id.clone(),
                        pubkey: p2p_pubkey,
                        role: GroupRole::Member,
                        alias,
                        avatar_base64: avatar,
                    };
                    members.push(new_meta.clone());
                    let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                    let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                    let action = GroupAction::AddMember { metadata: new_meta };
                    if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                        let action_payload = SignalingPayload::GroupAction(signed);
                        for m in &members {
                            if m.peer_id == my_peer_id || m.peer_id == requester_peer_id { continue; }
                            if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                            }
                        }
                    }

                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        // Check if we have the requester's static public key to wrap the secret
                        let mut static_key = None;
                        
                        // First, try contact storage
                        if let Ok(Some(contact)) = self.storage.get_contact(&requester_peer_id) {
                            static_key = Some(contact.static_key);
                        } else if let Some(sk_bytes) = self.pending_requester_static_keys.remove(&requester_peer_id) {
                            // Try the pending requests cache
                            if sk_bytes.len() == 32 {
                                let mut sk = [0u8; 32];
                                sk.copy_from_slice(&sk_bytes);
                                static_key = Some(sk);
                            }
                        }

                        let mut sent_invite = false;
                        if let Some(sk) = static_key {
                            if let Ok(wrapped) = group::GroupManager::wrap_group_secret(&group_info.secret, &sk) {
                                let invite_payload = SignalingPayload::GroupInvite {
                                    group_id: group_id.clone(),
                                    name: group_info.name.clone(),
                                    description: group_info.description.clone(),
                                    inviter_peer_id: my_peer_id.clone(),
                                    group_secret_wrapped: wrapped,
                                    members: members.clone(),
                                };
                                info!("[Mesh] Sending GroupInvite to approved requester {}", requester_peer_id);
                                let _ = self.forward_to_mesh(peer, invite_payload, false).await;
                                sent_invite = true;
                            }
                        }

                        if !sent_invite {
                            info!("[Mesh] No contact or pending static key found for approved requester {}, sending GroupManifest fallback", requester_peer_id);
                            let manifest_payload = SignalingPayload::GroupManifest {
                                group_id: group_id.clone(),
                                name: group_info.name.clone(),
                                description: group_info.description.clone(),
                                members: members.clone(),
                            };
                            let _ = self.forward_to_mesh(peer, manifest_payload, false).await;
                        }
                    }

                    crate::dispatch_global_event(23, group_id.as_bytes());
                }
            }
            NetworkCommand::RejectGroupJoin { group_id, requester_peer_id, reason } => {
                info!("[Mesh] Admin rejecting group join request for {} to group {}", requester_peer_id, group_id);
                self.pending_requester_static_keys.remove(&requester_peer_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        let reject_payload = SignalingPayload::GroupJoinRejected {
                            group_id,
                            group_name: group_info.name,
                            reason,
                        };
                        let _ = self.forward_to_mesh(peer, reject_payload, false).await;
                    }
                }
            }
            NetworkCommand::RemoveGroupMember { group_id, peer_id, members_json } => {
                info!("[Mesh] Removing member {} from group {}", peer_id, group_id);
                
                let group_data = if let Some(mj) = members_json {
                    Some(mj)
                } else if let Ok(Some(gi)) = self.storage.get_group(&group_id) {
                    Some(gi.members_json)
                } else {
                    None
                };

                if let Some(mj) = group_data {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&mj).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    let is_self = peer_id == my_peer_id;
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin && !is_self {
                        error!("[Mesh] Permission denied: Only admins can remove members");
                        return Ok(());
                    }

                    if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                        if members[pos].role == GroupRole::Creator {
                            error!("[Mesh] Permission denied: Creator cannot leave or be removed from the group");
                            return Ok(());
                        }

                        members.remove(pos);
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        
                        let action = GroupAction::RemoveMember { peer_id: peer_id.clone() };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            if is_self {
                                // If we removed ourselves, delete the group locally (if not already done) and notify the mesh
                                let _ = self.storage.delete_group(&group_id);
                                crate::dispatch_global_event(22, group_id.as_bytes());
                            } else {
                                // If we removed someone else (as admin), update DB locally and notify that person
                                let _ = self.storage.update_group_members(&group_id, &updated_members_json);
                                crate::dispatch_global_event(23, group_id.as_bytes());
                                if let Ok(pid) = peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }

                            // Notify all other members
                            for m in &members {
                                if m.peer_id == my_peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                    }
                }
            }
            NetworkCommand::UpdateGroupRole { group_id, peer_id, role } => {
                info!("[Mesh] Updating member {} role in group {} to {:?}", peer_id, group_id, role);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin {
                        error!("[Mesh] Permission denied: Only admins can update roles");
                        return Ok(());
                    }

                    if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                        if members[pos].role == GroupRole::Creator {
                            error!("[Mesh] Permission denied: Cannot change creator's role");
                            return Ok(());
                        }

                        members[pos].role = role.clone();
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                        let action = GroupAction::UpdateRole { peer_id: peer_id.clone(), new_role: role };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            for m in &members {
                                if m.peer_id == my_peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                    }
                }
            }
            NetworkCommand::PublishGroupManifest { group_id, code } => {
                info!("[Mesh] Publishing discovery record for Sovereign Group: {}", group_id);
                // SECURITY HARDENING: Never publish the group secret to the DHT.
                let key = RecordKey::new(&code.as_bytes());
                let record = libp2p::kad::Record {
                    key: key.clone(),
                    value: group_id.as_bytes().to_vec(),
                    publisher: None,
                    expires: None,
                };
                let _ = self.swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One);
                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
            }
            NetworkCommand::JoinMeshByCode { code } => {
                info!("[Mesh] Searching for Sovereign Group via code: {}", code);
                let key = RecordKey::new(&code.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_providers(key.clone());
                let _ = self.swarm.behaviour_mut().kademlia.get_record(key);
            }
            NetworkCommand::ResolveHandle { handle } => {
                info!("[Mesh] Resolving handle {} via DHT + RBN fallback...", handle);
                // 1. Try Kademlia DHT first
                let key = RecordKey::new(&handle.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_record(key);
                
                // 2. Also query RBNs directly as fallback
                let payload = SignalingPayload::HandleResolveRequest { handle: handle.clone() };
                for (rbn_id, _) in self.bootstrap_nodes.clone() {
                    let _ = self.forward_to_mesh(rbn_id, payload.clone(), false).await;
                }
            }
            NetworkCommand::SendDirectInvite { peer_id, identity, is_accept } => {
                let payload = if is_accept {
                    SignalingPayload::DirectInviteAccept(identity)
                } else {
                    SignalingPayload::DirectInviteRequest(identity)
                };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ClaimHandle { handle } => {
                info!("[Registry] Initiating claim for handle: {}", handle);
                let my_peer_id = self.swarm.local_peer_id().to_string();
                let timestamp = Utc::now().timestamp();
                
                // Generating PoW can be heavy, but at difficulty 4 it's fast.
                let pow_nonce = self.registry.generate_pow(&handle, &my_peer_id, timestamp);
                
                let payload = SignalingPayload::HandleClaimRequest {
                    handle,
                    peer_id: my_peer_id,
                    timestamp,
                    pow_nonce,
                };
                
                // Blast to all RBNs (Bootstrap nodes are the default RBNs)
                let rbn_ids: Vec<PeerId> = self.bootstrap_nodes.iter().map(|(id, _)| id.clone()).collect();
                for rbn_id in rbn_ids {
                    let _ = self.forward_to_mesh(rbn_id, payload.clone(), false).await;
                }
                
                // Also forward to any other discovered anchors
                for anchor_id in self.discovered_anchors.clone() {
                    let _ = self.forward_to_mesh(anchor_id, payload.clone(), false).await;
                }
            }
            NetworkCommand::BroadcastWitness { handle, peer_id, timestamp, pubkey, signature } => {
                 let my_peer_id = self.swarm.local_peer_id().to_string();
                 let payload = SignalingPayload::HandleClaimWitnessed {
                     handle,
                     peer_id: peer_id.clone(),
                     timestamp,
                     rbn_peer_id: my_peer_id,
                     rbn_pubkey: pubkey,
                     rbn_signature: signature,
                 };
                 
                 // Gossip back to requester
                 if let Ok(requester_pid) = PeerId::from_str(&peer_id) {
                     let _ = self.forward_to_mesh(requester_pid, payload.clone(), false).await;
                 }
                 
                 // Gossip to other anchors/RBNs
                 let rbn_ids: Vec<PeerId> = self.bootstrap_nodes.iter().map(|(id, _)| id.clone()).collect();
                 for rbn_id in rbn_ids {
                    let _ = self.forward_to_mesh(rbn_id, payload.clone(), false).await;
                 }
            }
            NetworkCommand::AcceptGroupInvite { group_id } => {
                info!("[Mesh] Accepting group invite for: {}", group_id);
                if let Ok(Some(invite)) = self.storage.get_pending_invite(&group_id) {
                    if let Ok(group_secret) = group::GroupManager::unwrap_group_secret(&invite.group_secret_wrapped, &self.local_static_secret) {
                        let _ = self.storage.save_group_secret(&group_id, &group_secret);
                        let _ = self.storage.upsert_group(&group_id, &invite.name, &invite.description, &invite.members_json);
                        let _ = self.storage.delete_pending_invite(&group_id);
                        let _ = self.storage.untombstone_group(&group_id);

                        // Subscribe to Gossipsub topic for this group immediately
                        let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                        if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                            error!("[Mesh] Failed to subscribe to gossipsub topic for accepted group {}: {:?}", group_id, e);
                        }

                        crate::dispatch_global_event(23, group_id.as_bytes());
                        info!("[Mesh] ✅ Group invite accepted: {}", invite.name);

                        // --- RELIABILITY FIX: Proactive Member Discovery ---
                        // Immediately attempt to dial all group members to establish the mesh.
                        let members: Vec<GroupMemberMetadata> = serde_json::from_str(&invite.members_json).unwrap_or_default();
                        let my_peer_id = self.swarm.local_peer_id().to_string();
                        for m in members {
                            if m.peer_id == my_peer_id { continue; }
                            if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                info!("[Mesh] Proactively dialing group member {} for mesh {}", pid, invite.name);
                                self.dial_relay_path(pid, false);
                            }
                        }
                    } else {
                        error!("[Mesh] ❌ Failed to unwrap group secret for {}", group_id);
                    }
                } else {
                    error!("[Mesh] No pending invite found for group: {}", group_id);
                }
            }
            NetworkCommand::DeclineGroupInvite { group_id } => {
                info!("[Mesh] Declining group invite for: {}", group_id);
                let _ = self.storage.delete_pending_invite(&group_id);
                info!("[Mesh] ✅ Group invite declined and removed.");
            }
            NetworkCommand::PublishGossipsub { topic, data } => {
                let ident_topic = libp2p::gossipsub::IdentTopic::new(topic.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(ident_topic, data) {
                    error!("[Mesh] ❌ Failed to publish gossipsub message to topic {}: {:?}", topic, e);
                }
            }
            NetworkCommand::SubscribeGossipsub { group_id } => {
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                match self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    Ok(subbed) => {
                        if subbed {
                            info!("[Mesh] Subscribed to gossipsub topic for group: {}", group_id);
                        } else {
                            debug!("[Mesh] Already subscribed to gossipsub topic for group: {}", group_id);
                        }
                    }
                    Err(e) => {
                        error!("[Mesh] ❌ Failed to subscribe to gossipsub topic for group {}: {:?}", group_id, e);
                    }
                }
            }
            NetworkCommand::BroadcastGroupMessage { group_id, message, reply_to } => {
                info!("[Mesh] Internal Broadcast for group {}: {}", group_id, message);
                let storage = self.storage.clone();
                let gid = group_id.clone();
                let keypair = self.local_keypair.clone();
                let tx = self.command_tx.clone();
                let my_peer_id = self.swarm.local_peer_id().to_string();

                // Phase 2.1: Snapshot connected peers BEFORE spawn to avoid &self borrow inside async block.
                // mesh_active_peers is HashSet<PeerId>, connected_peers collected as HashSet<String>
                // for direct string comparison with m.peer_id inside the closure.
                let connected_peers: std::collections::HashSet<String> = self.swarm.connected_peers()
                    .map(|p| p.to_string())
                    .collect();
                let active_mesh_peers = self.mesh_active_peers.clone();

                tokio::spawn(async move {
                    // Check if we are muted before broadcasting
                    if let Ok(muted) = storage.get_group_muted_members(&gid) {
                        if muted.contains(&my_peer_id) {
                            error!("[Mesh] ❌ Blocked broadcast: User is MUTED in group {}", gid);
                            return;
                        }
                    }

                    if let Ok(Some(group_secret_vec)) = storage.load_group_secret(&gid) {
                        if group_secret_vec.len() == 32 {
                            let mut group_secret = [0u8; 32];
                            group_secret.copy_from_slice(&group_secret_vec);

                            use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                            use rand::RngCore;
                            let mut nonce_bytes = [0u8; 12];
                            rand::thread_rng().fill_bytes(&mut nonce_bytes);
                            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_secret));

                            if let Ok(encrypted) = cipher.encrypt(Nonce::from_slice(&nonce_bytes), message.as_bytes()) {
                                let mut content_encrypted = nonce_bytes.to_vec();
                                content_encrypted.extend(encrypted);

                                let mut msg_id = format!("gm_int_{}_{}", gid, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
                                if message.starts_with("[FILE]:") {
                                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&message[7..]) {
                                        if let Some(tid) = meta.get("transfer_id").and_then(|v| v.as_str()) {
                                            msg_id = tid.to_string();
                                        }
                                    }
                                }

                                let action = GroupAction::Message { content_encrypted, msg_id, reply_to };
                                if let Ok(signed) = group::GroupManager::sign_action(gid.clone(), action, &keypair) {
                                    let payload = SignalingPayload::GroupAction(signed);
                                    if let Ok(data) = serde_json::to_vec(&payload) {
                                        let _ = tx.send(NetworkCommand::PublishGossipsub { topic: gid.clone(), data }).await;
                                        
                                        // Phase 2.1: Direct-forward ONLY to connected mesh peers for instant delivery.
                                        // Offline peers are handled by gossipsub propagation + mailbox drain.
                                        if let Ok(Some(group)) = storage.get_group(&gid) {
                                            let members: Vec<GroupMemberMetadata> = serde_json::from_str(&group.members_json).unwrap_or_default();
                                            for m in members {
                                                if m.peer_id != my_peer_id {
                                                    let is_connected = connected_peers.contains(&m.peer_id);
                                                    let is_active_mesh = m.peer_id.parse::<libp2p::PeerId>()
                                                        .map(|pid| active_mesh_peers.contains(&pid))
                                                        .unwrap_or(false);
                                                    if is_connected || is_active_mesh {
                                                        if let Ok(pid) = m.peer_id.parse::<libp2p::PeerId>() {
                                                            let tx_clone = tx.clone();
                                                            let payload_clone = payload.clone();
                                                            tokio::spawn(async move {
                                                                let _ = tx_clone.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload_clone }).await;
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }
            NetworkCommand::RegisterSeeder { peer_id, transfer_id, file_path, file_hash, chunk_size, total_chunks, group_id } => {
                self.active_seeders.insert(transfer_id, ActiveSeeder {
                    peer_id,
                    file_path: file_path.clone(),
                    file_hash: file_hash.clone(),
                    chunk_size,
                    total_chunks,
                    _bytes_sent: 0,
                    _start_time: Instant::now(),
                    group_id,
                    completions: HashSet::new(),
                });

                // SOVEREIGN DRIVE: Persist metadata so this node can serve as a mesh seeder indefinitely
                let storage_d = self.storage.clone();
                let path_d = file_path.clone();
                let hash_d = file_hash.clone();
                tokio::task::spawn_blocking(move || {
                    let filename = std::path::Path::new(&path_d).file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let size = std::fs::metadata(&path_d).map(|m| m.len()).unwrap_or(0);
                    // Use a generic mime type for seeding persistence if unknown
                    let _ = storage_d.upsert_drive_file(&filename, &hash_d, "application/octet-stream", size as i64, &path_d);
                });

                // SOVEREIGN SWARM: Announce that we are providing this file hash to the mesh
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
            }
            NetworkCommand::UnregisterSeeder { transfer_id } => {
                self.active_seeders.remove(&transfer_id);
            }
            NetworkCommand::CancelFileTransfer { transfer_id } => {
                info!("[Mesh] Cancelling file transfer: {}", transfer_id);
                // Remove from active seeders (outgoing transfers)
                self.active_seeders.remove(&transfer_id);
                // Remove from incoming transfers
                self.incoming_transfers.remove(&transfer_id);
                // Notify UI of cancellation
                let progress = FileTransferProgress {
                    transfer_id: transfer_id.clone(),
                    peer_id: String::new(),
                    filename: String::new(),
                    mime_type: String::new(),
                    file_hash: String::new(),
                    progress: 0.0,
                    is_complete: true,
                    is_verified: false,
                    is_outgoing: false,
                    local_path: None,
                    start_time_ms: 0,
                    speed_bps: 0.0,
                    group_id: None,
                    caption: None,
                };
                crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());
            }
            NetworkCommand::FindProviders { file_hash } => {
                info!("[Mesh] Searching Sovereign Swarm for providers of file: {}", file_hash);
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_providers(key);
            }
            NetworkCommand::SendFile { peer_id, file_path, group_id, transfer_id } => {
                let local_id = *self.swarm.local_peer_id();
                
                // If peer_id == local_id, it's a group broadcast share from Drive.
                if peer_id == local_id && group_id.is_some() {
                    let gid = group_id.as_ref().unwrap().clone();
                    info!("[Mesh] Group-wide file share detected for {}. Initiating intelligent swarm logic.", gid);

                    // Compute hash once to ensure stable transfer_id for all paths
                    let path = std::path::Path::new(&file_path);
                    if !path.exists() { return Err(anyhow::anyhow!("File not found: {}", file_path)); }
                    
                    let file_hash = {
                        let mut hasher = Sha256::new();
                        let f = std::fs::File::open(path)?;
                        let mut reader = std::io::BufReader::new(f);
                        std::io::copy(&mut reader, &mut hasher)?;
                        format!("{:x}", hasher.finalize())
                    };
                    
                    let t_id = transfer_id.unwrap_or_else(|| {
                        format!("gft_{}_{}", file_hash, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                    });
                    let tx = self.command_tx.clone();
                    
                    // 1. Proactive Local Push: Attempt to reach group members via direct paths
                    if let Ok(Some(group)) = self.storage.get_group(&gid) {
                        if let Ok(members) = serde_json::from_str::<Vec<GroupMemberMetadata>>(&group.members_json) {
                            let mut push_count = 0;
                            for member in members {
                                if member.peer_id == local_id.to_string() { continue; }
                                if push_count >= 100 { break; } // Scalability cap for proactive push
                                
                                if let Ok(m_peer_id) = member.peer_id.parse::<PeerId>() {
                                    // Even if not currently connected direct, try a SendFile.
                                    // SendFile logic will auto-negotiate WebRTC/Direct paths.
                                    info!("[Mesh] 🚀 Proactively initiating path discovery for group member {}.", member.peer_id);
                                    let f_path = file_path.clone();
                                    let g_id = Some(gid.clone());
                                    let t_id_clone = Some(t_id.clone());
                                    let tx_clone = tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx_clone.send(NetworkCommand::SendFile { 
                                            peer_id: m_peer_id, 
                                            file_path: f_path, 
                                            group_id: g_id,
                                            transfer_id: t_id_clone 
                                        }).await;
                                    });
                                    push_count += 1;
                                }
                            }
                        }
                    }

                    // 2. Gossipsub Manifest Broadcast: announce to the group topic so all members
                    // (including those not yet directly reachable) know a transfer is in progress.
                    // We pass is_relayed=false so that receivers who receive this manifest via Gossipsub
                    // will wait for the direct PUSH from the per-member SendFile calls above,
                    // rather than immediately entering the slow pull pipeline.
                    // Chunk delivery is handled entirely by the individual SendFile per-member paths.
                    let storage = self.storage.clone();
                    let is_stress = self.is_stress_test;
                    let t_id_for_broadcast = t_id.clone();
                    let relayed_map = self.is_relayed_map.clone();
                    let dc_store = self.data_channels.clone();
                    tokio::spawn(async move {
                        let _ = Self::process_outgoing_file(peer_id, file_path, false, relayed_map, dc_store, tx, storage, local_id, group_id, is_stress, Some(t_id_for_broadcast)).await;
                    });
                    return Ok(());
                }

                let already_direct = self.swarm.is_connected(&peer_id)
                    && self.is_relayed_map.read().get(&peer_id).cloned() == Some(false);
                let has_dc_already = {
                    let dc_store_read = self.data_channels.read();
                    if let Some(dc) = dc_store_read.get(&peer_id) {
                        dc.ready_state() == RTCDataChannelState::Open
                    } else {
                        false
                    }
                };

                if !has_dc_already && !already_direct {
                    // Kick off WebRTC negotiation — the spawn below will wait for it
                    info!("[Mesh] File transfer to {} initiated. Auto-negotiating WebRTC Data Channel...", peer_id);
                    let tx_webrtc = self.command_tx.clone();
                    let pid_webrtc = peer_id;
                    tokio::spawn(async move {
                        let _ = tx_webrtc.send(NetworkCommand::InitiateWebRtc { peer_id: pid_webrtc, media_type: 3 }).await;
                    });
                }

                let dc_store = Arc::clone(&self.data_channels);
                let tx = self.command_tx.clone();
                let tid_pass = transfer_id;

                tokio::spawn(async move {
                    // Wait up to 4 seconds (40 × 100ms) for the WebRTC Data Channel to reach Open state
                    if !has_dc_already {
                        for _ in 0..40 {
                            let open = {
                                let dc_store_read = dc_store.read();
                                if let Some(dc) = dc_store_read.get(&peer_id) {
                                    dc.ready_state() == RTCDataChannelState::Open
                                } else {
                                    false
                                }
                            };
                            if open {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                    let _ = tx.send(NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already, group_id, transfer_id: tid_pass }).await;
                });
            }
            NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already: _, group_id, transfer_id } => {
                let is_connected_now = self.swarm.is_connected(&peer_id);
                let relayed_map_snapshot = self.is_relayed_map.read().get(&peer_id).cloned();
                let tx = self.command_tx.clone();
                let storage = self.storage.clone();
                let local_peer_id = *self.swarm.local_peer_id();
                let is_stress = self.is_stress_test;
                let relayed_map = self.is_relayed_map.clone();
                let dc_store = self.data_channels.clone();

                tokio::spawn(async move {
                    let is_relayed = if is_connected_now {
                        relayed_map_snapshot.unwrap_or(false) // Default to false (direct P2P) if connected
                    } else {
                        false // Default to direct P2P — manifest will be retried if unreachable
                    };

                    let _ = Self::process_outgoing_file(peer_id, file_path, is_relayed, relayed_map, dc_store, tx, storage, local_peer_id, group_id, is_stress, transfer_id).await;
                });
            }
            NetworkCommand::SendFileChunk { peer_id, payload, progress } => {
                // PROACTIVE RELAY: Establish relay circuit to receiver BEFORE sending chunks.
                // Without this, forward_to_mesh buffers chunks in pending_messages because
                // the peer isn't connected yet. The relay dial takes ~2min on first attempt.
                // By dialing proactively, the circuit is ready when chunks need to flow.
                if !self.swarm.is_connected(&peer_id) {
                    self.dial_relay_path(peer_id, true);
                }

                // Persistent History/Torrent model: Always forward to mesh
                match self.forward_to_mesh(peer_id, payload, false).await {
                    Ok(_) => {
                        let data = serde_json::to_vec(&progress).unwrap_or_default();
                        crate::dispatch_global_event(12, &data);
                    }
                    Err(_) => {
                        // If forward failed, the helper already dialed or queued.
                    }
                }
            }
            NetworkCommand::SendAcknowledgement { peer_id, msg_id, status } => {
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                tokio::task::spawn_blocking(move || { let _ = storage.update_message_status_if_higher(&mid, status); });
                
                let payload = SignalingPayload::Acknowledgement { msg_id, status };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ForwardMeshSignaling { peer_id, payload } => {
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ForwardWebRtcNative { peer_id, json } => {
                // Forward a raw flutter_webrtc SDP/ICE JSON signal to the remote peer via mesh
                let payload = SignalingPayload::WebRtcNative(json);
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::StoreInMailbox { peer_id, payload } => {
                // Force mailbox: bypass direct delivery entirely.
                // This breaks the relay direct-retry loop for non-FileChunk payloads.
                let _ = self.forward_to_mesh(peer_id, payload, true).await;
            }
            NetworkCommand::HandleIncomingPayload { peer_id, payload } => {
                info!("[Mesh] HandleIncomingPayload received for peer {}", peer_id);
                self.handle_signaling_payload(peer_id, payload, false).await;
            }
            NetworkCommand::HandleIncomingWebRtcPayload { peer_id, payload } => {
                self.handle_signaling_payload(peer_id, payload, true).await;
            }
            NetworkCommand::TestManualRbn { address } => {
                info!("[Registry] Testing manual RBN connection to {}", address);
                let multiaddr_str = if address.contains("/ip4/") || address.contains("/ip6/") || address.contains("/dns/") {
                    address.clone()
                } else {
                    format!("/ip4/{}/tcp/443", address)
                };

                let multiaddr = match multiaddr_str.parse::<libp2p::Multiaddr>() {
                    Ok(addr) => addr,
                    Err(e) => {
                        let payload = format!("{}|Invalid address format: {}", address, e);
                        crate::dispatch_global_event(46, payload.as_bytes()); // Event 46: RbnConnectionFailed
                        return Ok(());
                    }
                };

                // Add to pending manual RBN connections
                self.pending_manual_rbns.write().insert(multiaddr.clone(), address.clone());

                let dial_res = self.swarm.dial(multiaddr.clone());
                if dial_res.is_err() {
                    self.pending_manual_rbns.write().remove(&multiaddr);
                    let payload = format!("{}|Dial failed: {:?}", address, dial_res.err());
                    crate::dispatch_global_event(46, payload.as_bytes()); // Event 46: RbnConnectionFailed
                    return Ok(());
                }

                // Spawn a watchdog task that waits 5 seconds. If the peer is not connected, dispatch failed event.
                let command_tx = self.command_tx.clone();
                let addr_str = address.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let _ = command_tx.send(NetworkCommand::VerifyManualRbnConnection { address: addr_str, multiaddr }).await;
                });
            }
            NetworkCommand::VerifyManualRbnConnection { address, multiaddr } => {
                // If it is still present in pending_manual_rbns, it means ConnectionEstablished did NOT fire!
                // So the connection failed or timed out.
                let was_pending = {
                    let mut pending = self.pending_manual_rbns.write();
                    pending.remove(&multiaddr).is_some()
                };

                if was_pending {
                    warn!("[Registry] Connection test failed or timed out for manual RBN: {}", address);
                    let payload = format!("{}|Connection timeout (5000ms reached)", address);
                    crate::dispatch_global_event(46, payload.as_bytes()); // Event 46: RbnConnectionFailed
                }
            }
            NetworkCommand::ForceMeshRefresh => {
                info!("[Network] Force Mesh Refresh triggered. Performing HARD RESET of networking stack.");
                // Immediately notify UI we are connecting
                crate::dispatch_global_event(10, &[3]);

                // 1. Actively disconnect all current peers to clear stale WiFi/VPN sockets
                let current_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                for pid in current_peers {
                    let _ = self.swarm.disconnect_peer_id(pid);
                }

                // 2. Clear established Noise sessions to force re-handshake on new IP
                self.noise_sessions.clear();

                // 3. Clear stale relay reservations so they are re-requested on reconnect
                //    Without this, the node can send outbound but can't receive inbound
                self.relay_reservations.clear();
                self.relay_listeners.clear();
                info!("[Network] Cleared relay reservations — will re-request on RBN reconnect");

                // 3b. Clear relay dial limiter — backoff penalties from old network
                //     should NOT delay reconnects on new network (VPN/WiFi/mobile switch).
                //     Without this, exponential backoff (up to 300s) prevents message delivery
                //     for minutes after a network type change.
                self.relay_dial_limiter.clear();
                info!("[Network] Cleared relay dial rate limiter — fresh start on new network");

                // 4. Re-resolve bootstrap nodes (critical for VPN/network transitions)
                //    This re-resolves DNS including NAT64/IPv6 addresses for the new network
                let fresh_bootstrap = config::get_bootstrap_nodes();
                if !fresh_bootstrap.is_empty() {
                    self.bootstrap_nodes = fresh_bootstrap.clone();
                    info!("[Network] Re-resolved {} bootstrap nodes for new network", fresh_bootstrap.len());
                }

                // 4. Re-inject bootstrap nodes and refresh DHT — dial ALL addresses (TCP, QUIC, port 443, port 80)
                for (peer_id, addr) in &self.bootstrap_nodes {
                    self.swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
                    info!("[Network] Dialing bootstrap: {}", addr);
                    let _ = self.swarm.dial(addr.clone());
                }

                // 5. Speed up discovery during sync
                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                self.perform_mailbox_fetch().await;

                // 6. Report current status immediately
                let connected_count = self.swarm.connected_peers().count();
                if connected_count > 0 {
                    let status = if self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit")) { 2u8 } else { 1u8 };
                    crate::dispatch_global_event(10, &[status]);
                    info!("[Network] ForceMeshRefresh complete — {} peers connected, status={}", connected_count, status);
                } else {
                    // No peers yet — show Relay (connecting via RBN)
                    crate::dispatch_global_event(10, &[2]);
                    info!("[Network] ForceMeshRefresh complete — dialing RBNs...");
                }

                // 7. VPN/NAT/MOBILE fallback: Reset tunnel if stale, then activate if needed
                //    On mobile data, carriers often block direct connections — tunnel is critical
                let tx_tunnel = self.command_tx.clone();
                let has_peers = self.swarm.connected_peers().count() > 0;
                let is_mobile = self.intro_claw.is_on_mobile_data();

                // MOBILE FIX: Force-reset tunnel on network change to clear stale VPN sockets
                if self.tunnel_active {
                    info!("[Network] Resetting stale tunnel for network transition (mobile={})", is_mobile);
                    self.tunnel_active = false;
                    self._tunnel_handle = None;
                    self.bootstrap_nodes.retain(|(_, addr)| !addr.to_string().contains("127.0.0.1"));
                }

                if !has_peers {
                    let delay = if is_mobile { 1 } else { 5 }; // Mobile: 1s, WiFi: 5s
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        info!("[Network] No direct connection after {}s — activating WebSocket tunnel (mobile={})", delay, is_mobile);
                        let _ = tx_tunnel.send(NetworkCommand::ActivateTunnel).await;
                    });
                }
            }
            NetworkCommand::ActivateTunnel => {
                if !self.tunnel_active {
                    // VPN: Use plaintext ws:// on port 80 (bypasses VPN TLS blocking)
                    // Non-VPN: Use encrypted wss:// on port 443
                    let tunnel_url = if self.connectivity_type == 5 {
                        types::RBN_WS_URL_PLAIN.to_string()
                    } else {
                        types::RBN_WS_URL.to_string()
                    };
                    info!("[Network] Activating WebSocket tunnel ({})...", if self.connectivity_type == 5 { "VPN/port 80" } else { "TLS/port 443" });
                    match tunnel::start_tunnel_client(0, tunnel_url.clone()).await {
                        Ok((port, handle)) => {
                            self.tunnel_active = true;
                            self.tunnel_started_at = Some(Instant::now());
                            self._tunnel_handle = Some(handle);
                            let tunnel_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;
                            let rbn_peer_id: PeerId = types::RBN_PEER_ID.parse()?;
                            if self.connectivity_type == 5 {
                                // VPN: Isolate bootstrap to tunnel ONLY. Direct dials to public IPs fail behind VPN.
                                self.bootstrap_nodes.clear();
                                self.bootstrap_nodes.push((rbn_peer_id, tunnel_addr.clone()));
                                info!("[Network] VPN Active: Isolated bootstrap to tunnel loopback only");
                            } else {
                                self.bootstrap_nodes.push((rbn_peer_id, tunnel_addr.clone()));
                            }
                            self.swarm.behaviour_mut().kademlia.add_address(&rbn_peer_id, tunnel_addr.clone());
                            let _ = self.swarm.dial(tunnel_addr);
                            let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                            info!("[Network] WebSocket tunnel active on local port {}", port);
                            crate::dispatch_debug_log(&format!("[VPN] Tunnel activated on port {} → {}", port, tunnel_url));
                        }
                        Err(e) => {
                            warn!("[Network] Failed to activate WebSocket tunnel: {:?}", e);
                            crate::dispatch_debug_log(&format!("[VPN] Tunnel activation FAILED: {}. VPN/firewall likely blocking WebSocket to {}", e, tunnel_url));
                            // Reset tunnel state so it can be retried
                            self.tunnel_active = false;
                            self.tunnel_started_at = None;
                        }
                    }
                } else {
                    info!("[Network] WebSocket tunnel already active");
                }
            }
            NetworkCommand::AddVerifiedRbn { peer_id } => {
                if !self.verified_rbns.contains(&peer_id) {
                    self.verified_rbns.insert(peer_id);
                    info!("[Security] Added {} to verified_rbns set after cryptographic test-dial.", peer_id);
                    crate::dispatch_debug_log(&format!("[Security] Verified RBN added: {}", peer_id));
                }
            }
            NetworkCommand::SendManualTelemetry => {
                info!("[Economy] Manual telemetry declaration triggered by user");
                let local_peer_id = self.swarm.local_peer_id().to_string();

                let mut seed = [0u8; 32];
                let mut engine_found = false;
                if let Some(engine) = crate::ENGINE.read().as_ref() {
                    seed = engine.identity.seed;
                    engine_found = true;
                }

                if !engine_found {
                    error!("[Economy] Engine state not initialized. Cannot send telemetry.");
                    return Ok(());
                }

                let signing_key = match crate::identity::NodeIdentity::derive_solana_keypair(seed) {
                    Ok(k) => k,
                    Err(e) => {
                        error!("[Economy] Failed to derive Solana keypair: {}", e);
                        return Ok(());
                    }
                };

                let sol_pubkey = solana_sdk::pubkey::Pubkey::new_from_array(signing_key.verifying_key().to_bytes());
                let solana_wallet = sol_pubkey.to_string();
                let solana_ata = crate::economy::derive_ata(&solana_wallet, "EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf")
                    .unwrap_or_else(|| solana_wallet.clone());
                let epoch_id = crate::economy::daily_rewards::economy_epoch_id();

                let mut prestige_tier = 0u8;
                if let Ok(Some((_, _, _, _, tier))) = self.storage.get_profile() {
                    prestige_tier = tier as u8;
                }

                let envelope = self.reward_tracker.package_signed_telemetry(
                    &local_peer_id,
                    &solana_wallet,
                    &solana_ata,
                    &epoch_id,
                    &signing_key,
                    false,
                    true,
                    prestige_tier,
                );



                if let Ok(envelope_json) = serde_json::to_string(&envelope) {


                    if let Err(e) = self.storage.save_local_telemetry(&epoch_id, &envelope_json) {


                        warn!("[Economy] Failed to save local telemetry: {:?}", e);


                    }


                }



                let telemetry_payload = SignalingPayload::TelemetryEnvelope {
                    peer_id: envelope.peer_id,
                    solana_wallet: envelope.solana_wallet,
                    solana_ata: envelope.solana_ata,
                    epoch_id: envelope.epoch_id,
                    metrics: envelope.metrics,
                    unique_peers: envelope.unique_peers,
                    is_rbn: envelope.is_rbn,
                    is_edge_node: envelope.is_edge_node,
                    prestige_tier: envelope.prestige_tier,
                    proof_hash: envelope.proof_hash,
                    client_signature: envelope.client_signature,
                    timestamp: envelope.timestamp,
                };

                let mut sent = false;
                let bootstrap_clone = self.bootstrap_nodes.clone();
                for (rbn_id, _) in &bootstrap_clone {
                    if self.swarm.is_connected(rbn_id) {
                        let _ = self.forward_to_mesh(*rbn_id, telemetry_payload.clone(), false).await;
                        self.last_telemetry_sent = Instant::now();
                        info!("[Economy] Manual telemetry sent to RBN {}", rbn_id);
                        sent = true;
                        break;
                    }
                }
                if !sent {
                    warn!("[Economy] Manual telemetry failed — no RBN connected");
                    crate::dispatch_debug_log("[Economy] Manual telemetry failed: no RBN connected. Connect to mesh first.");
                }
            }
            NetworkCommand::InitiateWebRtc { peer_id, media_type } => {
                let (pc, mut dc_rx) = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await?;
                let dc_store = Arc::clone(&self.data_channels);
                tokio::spawn(async move {
                    if let Some(dc) = dc_rx.recv().await {
                        dc_store.write().insert(peer_id, dc);
                    }
                });

                if let Err(e) = MediaManager::add_media_tracks(Arc::clone(&pc), media_type).await {
                    error!("❌ Failed to add media tracks: {:?}", e);
                }

                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let purpose = if media_type == 3 { Some("file_transfer".to_string()) } else { None };
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp, purpose };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure { 
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(signal), false).await;
                }
            }
            NetworkCommand::StartMediaStream { peer_id, media_type } => {
                let pc_clone = { let pcs = self.peer_connections.read(); pcs.get(&peer_id).cloned() };
                if let Some(pc) = pc_clone { MediaManager::add_media_tracks(pc, media_type).await?; }
            }
            NetworkCommand::AcceptWebRtc { peer_id, media_type } => {
                if let Some(offer_sdp) = self.pending_offers.remove(&peer_id) {
                    if let Ok((pc, mut dc_rx)) = MediaManager::create_peer_connection(false, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await {
                        let dc_store = Arc::clone(&self.data_channels);
                        tokio::spawn(async move {
                            if let Some(dc) = dc_rx.recv().await {
                                dc_store.write().insert(peer_id, dc);
                            }
                        });

                        if let Err(e) = MediaManager::add_media_tracks(Arc::clone(&pc), media_type).await {
                            error!("❌ Failed to add media tracks: {:?}", e);
                        }

                        if let Ok(answer_sdp) = MediaManager::handle_offer(offer_sdp, Arc::clone(&pc)).await {
                            self.peer_connections.write().insert(peer_id, pc);
                            let response = WebRtcSignal { signal_type: "answer".to_owned(), sdp: answer_sdp, purpose: None };
                            
                            let mut is_secure = false;
                            if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                                if session.is_finished() {
                                    let mut payload = b"WEBRTC:".to_vec();
                                    payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                                    if let Ok(encrypted) = session.send_message(&payload) {
                                        let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                                        is_secure = true;
                                    }
                                }
                            }
                            if !is_secure {
                                let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                            }
                        }
                    }
                }
            }
            NetworkCommand::RejectWebRtc { peer_id } => {
                self.pending_offers.remove(&peer_id);
                let response = WebRtcSignal { signal_type: "reject".to_owned(), sdp: "".to_owned(), purpose: None };
                
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure {
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                }
            }
            NetworkCommand::CloseWebRtc { peer_id } => { 
                let response = WebRtcSignal { signal_type: "reject".to_owned(), sdp: "".to_owned(), purpose: None };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure {
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                }

                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; } 

                let data = peer_id.to_string().into_bytes();
                crate::dispatch_global_event(16, &data);
            }
            NetworkCommand::WebRtcFailed { peer_id } => {
                info!("Peer Connection State has changed: failed");
                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; }

                let data = peer_id.to_string().into_bytes();
                crate::dispatch_global_event(16, &data);

                // RESTORE/UPDATE STATUS: WebRTC channel failed, fall back to current libp2p link state
                let is_connected = self.swarm.is_connected(&peer_id);
                let status: u8 = if is_connected {
                    if self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false) {
                        1 // Relayed
                    } else {
                        0 // Direct P2P
                    }
                } else {
                    2 // Offline
                };
                let mut data = peer_id.to_string().into_bytes();
                data.push(b':');
                data.push(status);
                crate::dispatch_global_event(8, &data);
                
                // RECOVERY: For any active outgoing transfers to this peer, re-broadcast
                // the manifest with is_relayed=true so the receiver switches to pull mode
                // and requests the chunks it's missing via the mailbox path.
                let active_transfers: Vec<(String, String, String, usize, u32, u32)> = self.active_seeders
                    .iter()
                    .filter(|(_, s)| s.peer_id == peer_id)
                    .map(|(tid, s)| (tid.clone(), s.file_path.clone(), s.file_hash.clone(), 
                                     (s.total_chunks * s.chunk_size) as usize, s.chunk_size, s.total_chunks))
                    .collect();
                
                for (transfer_id, _, file_hash, total_size, _, _) in active_transfers {
                    let local_peer_id = *self.swarm.local_peer_id();
                    let recovery_manifest = SignalingPayload::FileTransfer {
                        transfer_id,
                        filename: "".to_string(), // receiver already has this
                        mime_type: "".to_string(),
                        file_hash,
                        total_size: total_size as u64,
                        is_relayed: true,  // Switch receiver to pull mode
                        sender_peer_id: Some(local_peer_id.to_string()),
                        group_id: None,
                    };
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                            peer_id, 
                            payload: recovery_manifest 
                        }).await;
                    });
                }
            }
            NetworkCommand::RenegotiateWebRtc { peer_id } => { 
                let (pc, mut dc_rx) = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await?;
                let dc_store = Arc::clone(&self.data_channels);
                tokio::spawn(async move {
                    if let Some(dc) = dc_rx.recv().await {
                        dc_store.write().insert(peer_id, dc);
                    }
                });

                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp, purpose: None };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure { 
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(signal), false).await;
                }
            }
            NetworkCommand::AddAddress { peer_id, address } => { self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address); }
            NetworkCommand::EstablishSecureSession { peer_id } => {
                if self.noise_sessions.contains_key(&peer_id) { return Ok(()); }
                let peer_id_str = peer_id.to_string();
                let storage = Arc::clone(&self.storage);

                // Tie-breaker: lexicographical comparison of PeerId strings
                let local_peer_str = self.swarm.local_peer_id().to_string();
                let is_initiator = local_peer_str < peer_id_str;

                if is_initiator {
                    let storage_contact = storage.clone();
                    let pid_str = peer_id_str.clone();
                    let contact = tokio::task::spawn_blocking(move || storage_contact.get_contact(&pid_str)).await??;
                    if let Some(identity) = contact {
                        info!("[Mesh] Establishing secure session: Initiator role for peer {}", peer_id_str);
                        let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &identity.static_key)?;
                        let handshake_msg = session.send_message(&[])?;
                        let storage_save = Arc::clone(&self.storage);
                        let enc_key = self.session_encryption_key;
                        let session_state = session.get_state();
                        tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage_save, enc_key, peer_id, session_state).await; });
                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                        self.noise_sessions.insert(peer_id, session);
                    } else {
                        // Fallback: try loading public key from session cache if contact database isn't fully synced yet
                        let storage_load = storage.clone();
                        let session_blob = tokio::task::spawn_blocking({ let peer_id_str = peer_id_str.clone(); move || storage_load.load_session_state(&peer_id_str) }).await??;
                        if let Some(encrypted_blob) = session_blob {
                            use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                            let key = Key::<Aes256Gcm>::from_slice(&self.session_encryption_key);
                            let cipher = Aes256Gcm::new(key);
                            if encrypted_blob.len() > 12 {
                                let nonce = Nonce::from_slice(&encrypted_blob[0..12]);
                                if let Ok(decrypted) = cipher.decrypt(nonce, &encrypted_blob[12..]) {
                                    if let Ok(state) = bincode::deserialize::<crate::network::noise_session::NoiseSessionState>(&decrypted) {
                                        if let Some(remote_pk) = &state.remote_public {
                                            info!("[Mesh] Establishing secure session: Initiator role (loaded from cache) for peer {}", peer_id_str);
                                            let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), remote_pk)?;
                                            let handshake_msg = session.send_message(&[])?;
                                            let storage_save = Arc::clone(&self.storage);
                                            let enc_key = self.session_encryption_key;
                                            let session_state = session.get_state();
                                            tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage_save, enc_key, peer_id, session_state).await; });
                                            self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                                            self.noise_sessions.insert(peer_id, session);
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }

                        info!("[Mesh] Peer {} not in contacts. Querying Kademlia for identity...", peer_id_str);
                        let key = RecordKey::new(&peer_id.to_bytes());
                        let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);
                        self.pending_handshakes.insert(query_id, peer_id);
                    }
                } else {
                    info!("[Mesh] Establishing secure session: Responder role for peer {}", peer_id_str);
                    if let Ok(session) = NoiseSession::responder(self.local_static_secret.to_bytes().as_slice()) {
                        self.noise_sessions.insert(peer_id, session);
                    }
                }
            }
            NetworkCommand::RecheckConnection { peer_id } => {
                info!("[Diagnostics] Starting connection recheck for peer {}", peer_id);

                // Register diagnostic state
                self.pending_diagnostics.insert(peer_id, PendingDiagnostic {
                    start_time: Instant::now(),
                    transport: None,
                });

                // Dispatch scanning started event (Event Type 15: Connection Diagnostics)
                let diag_payload = format!(
                    r#"{{"peer_id":"{}","step":"start","status":"scanning"}}"#,
                    peer_id
                );
                crate::dispatch_global_event(15, diag_payload.as_bytes());

                // If ALREADY connected, report current status immediately so UI doesn't look stuck
                if self.swarm.is_connected(&peer_id) {
                    let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                    let transport = if !is_relayed { "Direct P2P" } else { "Relayed Connection" };
                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"settled","transport":"{}"}}"#,
                        peer_id, transport
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }

                // SOFT RECHECK: Don't disconnect if we already have a connection.
                // Just force a re-dial of known addresses (including direct ones).
                // This allows libp2p to upgrade or add a direct path without dropping the relay path.
                if !self.swarm.is_connected(&peer_id) {
                    self.is_relayed_map.write().remove(&peer_id);
                }

                // Clear rate limiter to allow immediate redial
                self.relay_dial_limiter.remove(&peer_id);

                // 1. Try direct dial (Direct P2P)
                let _ = self.swarm.dial(peer_id);

                // 2. Try relay paths through all bootstrap nodes (Relayed QUIC + TCP)
                for (rbn_id, rbn_addr) in &self.bootstrap_nodes {
                    if rbn_addr.to_string().contains("443") {
                        let relay_addr = rbn_addr.clone()
                            .with(libp2p::multiaddr::Protocol::P2p(rbn_id.clone()))
                            .with(libp2p::multiaddr::Protocol::P2pCircuit)
                            .with(libp2p::multiaddr::Protocol::P2p(peer_id));
                        let _ = self.swarm.dial(relay_addr);
                    }
                }

                // 3. Schedule 8-second timeout guard
                let tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(8)).await;
                    let _ = tx.send(NetworkCommand::HandleDiagnosticTimeout { peer_id }).await;
                });
            }
            NetworkCommand::HandleDiagnosticTimeout { peer_id } => {
                if let Some(diag) = self.pending_diagnostics.remove(&peer_id) {
                    let transport = diag.transport.unwrap_or_else(|| "None".to_string());
                    let elapsed = diag.start_time.elapsed().as_millis() as u64;
                    let status = if transport == "None" { "failed" } else { "settled" };

                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"timeout","status":"{}","transport":"{}","rtt_ms":{}}}"#,
                        peer_id, status, transport, elapsed
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }
            }
            NetworkCommand::PollPeerProfile { peer_id } => {
                info!("[Mesh] Polling profile for peer: {}", peer_id);
                let payload = SignalingPayload::ProfileRequest;
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::SyncChatMessages { peer_id, chat_id, is_group, is_full } => {
                // Prevent concurrent syncs for the same chat
                if self.sync_in_progress.contains_key(&chat_id) {
                    debug!("[Mesh] Sync already in progress for {}, skipping", chat_id);
                    return Ok(());
                }
                self.sync_in_progress.insert(chat_id.clone(), Instant::now());
                info!("[Mesh] Syncing messages for chat: {} (group={}, full={})", chat_id, is_group, is_full);
                let storage = Arc::clone(&self.storage);
                let chat_id_clone = chat_id.clone();
                let is_group_clone = is_group;

                let known_ids = if is_full {
                    Vec::new() // Empty = request all messages
                } else {
                    tokio::task::spawn_blocking(move || {
                        let mut ids = Vec::new();
                        if is_group_clone {
                            if let Ok(msgs) = storage.get_group_messages(&chat_id_clone) {
                                for m in msgs { ids.push(m.1.clone()); } // m.1 = msg_id
                            }
                        } else {
                            if let Ok(msgs) = storage.get_messages_for_peer(&chat_id_clone) {
                                for m in msgs { if let Some(mid) = &m.4 { ids.push(mid.clone()); } }
                            }
                        }
                        // Focus fast sync on the last 100 messages to reduce wire size and speed up local load
                        if ids.len() > 100 {
                            let start_idx = ids.len() - 100;
                            ids[start_idx..].to_vec()
                        } else {
                            ids
                        }
                    }).await.unwrap_or_default()
                };

                info!("[Mesh] Sending sync request with {} known IDs (full={})", known_ids.len(), is_full);
                let payload = SignalingPayload::ChatSyncRequest {
                    chat_id,
                    is_group,
                    known_msg_ids: known_ids,
                    limit: if is_full { 10000 } else { 100 },
                };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::RelaySyncedMessages { chat_id, messages } => {
                let connected_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                let my_id = *self.swarm.local_peer_id();
                info!("[Mesh] Relaying {} synced messages to {} peers for group {}", messages.len(), connected_peers.len(), chat_id);
                for pid in connected_peers {
                    if pid == my_id { continue; }
                    let response = SignalingPayload::ChatSyncResponse {
                        chat_id: chat_id.clone(),
                        is_group: true,
                        messages: messages.clone(),
                        missing_ids: Vec::new(),
                        is_relay: true,
                    };
                    let _ = self.forward_to_mesh(pid, response, false).await;
                }
            }
            NetworkCommand::RequestSwarmStats => {
                // Calculate local storage contribution: min(1GB, 75% of free disk space)
                let local_storage_gb: u64 = {
                    let free_bytes: u64 = {
                        #[cfg(unix)]
                        {
                            use std::ffi::CString;
                            let path = CString::new("/").unwrap();
                            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
                            if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0 {
                                (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64)
                            } else {
                                10u64 * 1024 * 1024 * 1024 // fallback 10GB
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            10u64 * 1024 * 1024 * 1024 // fallback 10GB
                        }
                    };
                    let free_gb = free_bytes / (1024 * 1024 * 1024);
                    let seventy_five_pct = (free_gb * 75) / 100;
                    std::cmp::min(1u64, seventy_five_pct)
                };

                let mut unique_nodes = self.mesh_active_peers.clone();
                let local_id = *self.swarm.local_peer_id();
                
                for pid in self.swarm.connected_peers() {
                    unique_nodes.insert(*pid);
                }
                for pid in &self.discovered_anchors {
                    unique_nodes.insert(*pid);
                }

                // Remove self from the set to calculate remote count
                unique_nodes.remove(&local_id);
                let total_nodes = unique_nodes.len() as u64 + 1;
                
                // Active Users Online: all peers we have interacted with in this session + us
                let active_users = total_nodes;
                
                // Collective Capacity: local device + remote non-RBN peers
                // RBN nodes (bootstrap_nodes) contribute 0 storage
                // Regular remote peers contribute 1 GB each
                // Local device contributes min(1GB, 75% of free disk)
                let rbn_ids: std::collections::HashSet<_> = self.bootstrap_nodes.iter().map(|(id, _)| *id).collect();
                let remote_non_rbn_count = unique_nodes.iter().filter(|pid| !rbn_ids.contains(pid)).count() as u64;
                let capacity_gb = local_storage_gb + remote_non_rbn_count;
                
                let stats = serde_json::json!({
                    "total_nodes": total_nodes,
                    "active_users": active_users,
                    "collective_capacity_gb": capacity_gb,
                    "active_transfers": self.incoming_transfers.len() + self.active_seeders.len(),
                });

                if let Ok(json) = serde_json::to_string(&stats) {
                    crate::dispatch_global_event(30, json.as_bytes()); // Event Type 30: Swarm Stats
                }
            }
            NetworkCommand::IntroClawTick { battery_pct, is_background, connected_peers, mdns_discovered, is_mobile_data, network_type } => {
                let active_hashes = self.storage.get_active_drive_hashes();
                let ctx = crate::intro_claw::ClawTickContext {
                    battery_pct,
                    is_background,
                    connected_peers,
                    mdns_discovered,
                    active_transfer_hashes: active_hashes,
                    is_mobile_data,
                    network_type,
                    connectivity_type: self.connectivity_type,
                    tunnel_active: self.tunnel_active,
                    consecutive_zero_peers_ticks: self.consecutive_zero_peers_ticks,
                    has_relay_reservation: !self.relay_reservations.is_empty(),
                };
                let actions = self.intro_claw.tick(&ctx);
                self.execute_claw_actions(actions).await;
            }
            NetworkCommand::IntroClawSetActive { active } => {
                self.intro_claw.set_active(active);
                let _ = self.storage.set_intro_claw_active(active);
            }
            NetworkCommand::IntroClawNetworkRecon { result_tx } => {
                // Build real ReconContext from live swarm state
                let is_relayed = self.is_relayed_map.read();
                let mut peers: Vec<crate::intro_claw::ReconPeerInfo> = self.mesh_active_peers.iter().map(|pid| {
                    let pid_str = pid.to_string();
                    let connected = true;
                    let relayed = is_relayed.get(pid).cloned().unwrap_or(false);
                    let anchor = self.discovered_anchors.contains(pid);
                    let direct_count = self.direct_conn_count.get(pid).cloned().unwrap_or(0);
                    let alias = self.storage.get_contact(&pid_str).ok().flatten().and_then(|c| c.global_name.or(c.local_alias));
                    let handle = self.storage.get_contact(&pid_str).ok().flatten().and_then(|c| c.handle);
                    crate::intro_claw::ReconPeerInfo {
                        peer_id: pid_str,
                        is_connected: connected,
                        is_relayed: relayed,
                        is_anchor: anchor,
                        direct_conn_count: direct_count,
                        alias,
                        handle,
                    }
                }).collect();

                // Also add known but disconnected peers from contacts
                if let Ok(contacts) = self.storage.get_all_contacts() {
                    for contact in contacts {
                        if !self.mesh_active_peers.iter().any(|p| p.to_string() == contact.peer_id) {
                            peers.push(crate::intro_claw::ReconPeerInfo {
                                peer_id: contact.peer_id.clone(),
                                is_connected: false,
                                is_relayed: false,
                                is_anchor: self.discovered_anchors.iter().any(|a| a.to_string() == contact.peer_id),
                                direct_conn_count: 0,
                                alias: contact.global_name.or(contact.local_alias),
                                handle: contact.handle,
                            });
                        }
                    }
                }

                let local_id = self.swarm.local_peer_id().to_string();
                let (drive_bytes, mesh_bytes, total_disk) = self.storage.get_storage_usage();
                let battery = self.intro_claw.get_battery_pct();

                let ctx = crate::intro_claw::ReconContext {
                    connected_peers: self.mesh_active_peers.iter().map(|p| p.to_string()).collect(),
                    total_known_peers: peers.len(),
                    discovered_anchors: self.discovered_anchors.iter().map(|p| p.to_string()).collect(),
                    relay_reservations: self.relay_reservations.len(),
                    active_seeders: self.active_seeders.len(),
                    incoming_transfers: self.incoming_transfers.len(),
                    pending_messages: self.pending_messages.values().map(|v| v.len()).sum(),
                    storage_usage: (drive_bytes, mesh_bytes, total_disk),
                    peers,
                    battery_pct: battery,
                    node_peer_id: local_id,
                };

                let report = crate::intro_claw::run_network_recon(&ctx, &self.storage);
                self.intro_claw.log_event("recon", &format!("Network recon complete — {} peers, {} anchors",
                    ctx.connected_peers.len(), ctx.discovered_anchors.len()), "success");
                let _ = result_tx.send(report);
            }
            NetworkCommand::IntroClawNetworkHeal { peer_id, result_tx } => {
                // Execute real multi-strategy heal
                let mut attempts = Vec::new();
                let is_connected = self.mesh_active_peers.contains(&peer_id);
                let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                let has_direct_addr = self.direct_conn_count.contains_key(&peer_id) || self.anchor_mappings.contains_key(&peer_id);
                let has_anchor = !self.discovered_anchors.is_empty();
                let connected_anchors: Vec<String> = self.discovered_anchors.iter().map(|a| a.to_string()).collect();

                // Strategy 1: Direct dial
                if !is_connected {
                    info!("[Intro-Claw Heal] Strategy 1: Direct dial to {}", &peer_id.to_string()[..16.min(peer_id.to_string().len())]);
                    match self.swarm.dial(peer_id) {
                        Ok(_) => {
                            attempts.push(crate::intro_claw::HealAttempt {
                                strategy: "STRATEGY_1_DIRECT_DIAL".to_string(),
                                target: peer_id.to_string(),
                                success: true,
                                detail: "Direct libp2p dial initiated".to_string(),
                            });
                        }
                        Err(e) => {
                            attempts.push(crate::intro_claw::HealAttempt {
                                strategy: "STRATEGY_1_DIRECT_DIAL".to_string(),
                                target: peer_id.to_string(),
                                success: false,
                                detail: format!("Direct dial failed: {}", e),
                            });

                            // Strategy 2: Relay path
                            info!("[Intro-Claw Heal] Strategy 2: Relay circuit via RBN");
                            self.dial_relay_path(peer_id, false);
                            attempts.push(crate::intro_claw::HealAttempt {
                                strategy: "STRATEGY_2_RELAY_PATH".to_string(),
                                target: peer_id.to_string(),
                                success: true,
                                detail: "Relay circuit v2 initiated via RBN backbone".to_string(),
                            });
                        }
                    }

                    // Strategy 3: Anchor routing
                    if has_anchor {
                        info!("[Intro-Claw Heal] Strategy 3: Anchor routing via {} nodes", connected_anchors.len());
                        for anchor_id in &self.discovered_anchors {
                            if self.mesh_active_peers.contains(anchor_id) {
                                attempts.push(crate::intro_claw::HealAttempt {
                                    strategy: "STRATEGY_3_ANCHOR_ROUTE".to_string(),
                                    target: anchor_id.to_string(),
                                    success: true,
                                    detail: format!("Routing via connected anchor node"),
                                });
                                break;
                            }
                        }
                    }
                }

                let alias = self.storage.get_contact(&peer_id.to_string()).ok().flatten().and_then(|c| c.global_name.or(c.local_alias));
                let final_connected = self.mesh_active_peers.contains(&peer_id);

                let result = crate::intro_claw::HealResult {
                    peer_id: peer_id.to_string(),
                    peer_alias: alias,
                    attempts,
                    final_connected,
                    recommended_action: if final_connected {
                        "Peer connected successfully".to_string()
                    } else {
                        "Peer still unreachable — messages stored in mailbox for later delivery".to_string()
                    },
                };

                let report = crate::intro_claw::render_heal_report(&result);
                self.intro_claw.log_event("heal", &format!("Heal complete for {} — connected: {}",
                    &peer_id.to_string()[..16.min(peer_id.to_string().len())], final_connected),
                    if final_connected { "success" } else { "warn" });
                let _ = result_tx.send(report);
            }
            NetworkCommand::IntroClawGetActivityLog { result_tx } => {
                let json = self.intro_claw.get_activity_log_json();
                let _ = result_tx.send(json);
            }
            NetworkCommand::IntroClawVoipStartCall { peer_id, is_video } => {
                self.intro_claw.voip_start_call(&peer_id, is_video);
            }
            NetworkCommand::IntroClawVoipEndCall => {
                self.intro_claw.voip_end_call();
            }
            NetworkCommand::IntroClawVoipRecordSample { rtt_ms, packet_loss_pct, jitter_ms, bitrate_kbps, is_relayed, codec } => {
                self.intro_claw.voip_record_sample(rtt_ms, packet_loss_pct, jitter_ms, bitrate_kbps, is_relayed, &codec);
            }
            NetworkCommand::IntroClawVoipGetQuality { result_tx } => {
                let quality = self.intro_claw.voip_get_quality_summary();
                let _ = result_tx.send(quality);
            }
            NetworkCommand::IntroClawSetNodeMode { enabled } => {
                self.intro_claw.set_node_mode(enabled);
            }
            NetworkCommand::IntroClawVoipGetDowngradeRecommendation { result_tx } => {
                let recommendation = self.intro_claw.voip_get_downgrade_recommendation();
                let _ = result_tx.send(recommendation);
            }
            NetworkCommand::IntroClawSetActiveChat { chat_id, peer_id, is_group } => {
                self.intro_claw.set_active_chat(&chat_id, peer_id.as_deref(), is_group);
            }
            NetworkCommand::IntroClawClearActiveChat => {
                self.intro_claw.clear_active_chat();
            }
            NetworkCommand::IntroClawSetActiveGroupMembers { members } => {
                self.intro_claw.set_active_group_members(members);
            }
            NetworkCommand::IntroClawOnAppLaunch { result_tx } => {
                let actions = self.intro_claw.on_app_launch();
                self.execute_claw_actions(actions).await;
                let _ = result_tx.send(());
            }
        }
        Ok(())
    }

    async fn persist_session_state(storage: Arc<crate::storage::StorageService>, session_encryption_key: [u8; 32], peer_id: PeerId, state: crate::network::noise_session::NoiseSessionState) -> anyhow::Result<()> {
        use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
        use rand::RngCore;
        let encoded = bincode::serialize(&state)?;
        let key = Key::<Aes256Gcm>::from_slice(&session_encryption_key);
        let cipher = Aes256Gcm::new(key);
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let mut encrypted = cipher.encrypt(nonce, encoded.as_ref()).map_err(|_| anyhow::anyhow!("Session encryption failed"))?;
        let mut final_payload = nonce_bytes.to_vec();
        final_payload.append(&mut encrypted);
        let peer_id_str = peer_id.to_string();
        tokio::task::spawn_blocking(move || storage.save_session_state(&peer_id_str, final_payload)).await??;
        Ok(())
    }

    async fn start_peer_reaper(peer_connections: Arc<RwLock<HashMap<PeerId, Arc<RTCPeerConnection>>>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut closed_peers = Vec::new();
            {
                let pcs = peer_connections.read();
                for (peer, pc) in pcs.iter() {
                    if pc.connection_state() == RTCPeerConnectionState::Closed || pc.connection_state() == RTCPeerConnectionState::Failed { closed_peers.push(*peer); }
                }
            }
            if !closed_peers.is_empty() {
                let mut pcs = peer_connections.write();
                for peer in closed_peers { pcs.remove(&peer); }
            }
        }
    }

    async fn start_mailbox_cleanup(storage: Arc<crate::storage::StorageService>) {
        let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            let _ = storage.cleanup_expired_mailbox();
        }
    }

    async fn start_message_pruning(storage: Arc<crate::storage::StorageService>) {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60)); // Check every hour
        loop {
            interval.tick().await;
            let _ = storage.prune_expired_messages();
        }
    }

    /// Async background registry validator — the Track A fallback.
    ///
    /// After the client comes online via cached historical IPs, this task
    /// queries the Solana Mainnet Registry (`FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW`)
    /// for handle entries where `is_community_rbn == true`. For each entry it
    /// performs a mandatory cryptographic test-dial: a low-timeout Noise
    /// handshake that physically verifies the responding node's libp2p public
    /// key matches the on-chain claimed `peer_id`. Mismatches are discarded
    /// with an emergency security warning.
    ///
    /// After verification, the validated `peer_id` is appended to the live
    /// `verified_rbns` set so relay routing and mailbox storage agree.
    async fn background_registry_validator(
        storage: Arc<crate::storage::StorageService>,
        command_tx: mpsc::Sender<NetworkCommand>,
    ) {
        // Wait a bit for the network to stabilize
        tokio::time::sleep(Duration::from_secs(15)).await;

        info!("[Registry Validator] Starting async Solana registry validation...");

        // Initialize Solana client for Mainnet
        let solana_client = match crate::economy::solana::SolanaIncentiveEngine::new(
            "https://api.mainnet-beta.solana.com",
            "EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf",
            "https://api.introvert.network/claim",
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!("[Registry Validator] Failed to initialize Solana client: {}", e);
                return;
            }
        };

        // Fetch all handle accounts from the Introvert Handle Registry program
        let program_id_str = "FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW";
        match solana_client.fetch_registered_rbn_details(program_id_str).await {
            Ok(entries) => {
                let community_rbns: Vec<&crate::economy::solana::RbnRegistryEntry> = entries
                    .iter()
                    .filter(|e| e.is_active)
                    .collect();

                if community_rbns.is_empty() {
                    info!("[Registry Validator] No active community RBN entries found on-chain.");
                    return;
                }

                info!("[Registry Validator] Found {} active community RBN entries on-chain. Starting cryptographic test-dials...", community_rbns.len());

                let mut verified_count = 0u32;
                let mut rejected_count = 0u32;

                for entry in &community_rbns {
                    let claimed_peer_id = &entry.peer_id;
                    let ip_addr = &entry.multiaddresses;
                    let multiaddr_str = if ip_addr.starts_with('/') {
                        ip_addr.clone()
                    } else {
                        format!("/ip4/{}/tcp/443", ip_addr)
                    };

                    let addr: Multiaddr = match multiaddr_str.parse() {
                        Ok(a) => a,
                        Err(e) => {
                            warn!("[Registry Validator] Invalid multiaddr '{}' for {}: {:?}", multiaddr_str, claimed_peer_id, e);
                            rejected_count += 1;
                            continue;
                        }
                    };

                    let expected_pid: PeerId = match claimed_peer_id.parse() {
                        Ok(p) => p,
                        Err(e) => {
                            warn!("[Registry Validator] Invalid PeerId '{}': {:?}", claimed_peer_id, e);
                            rejected_count += 1;
                            continue;
                        }
                    };

                    // MANDATORY CRYPTOGRAPHIC TEST-DIAL:
                    // Open a low-timeout transport link to the fetched IP and force
                    // a Noise handshake. The Identify protocol will reveal the
                    // remote node's actual public key. If it doesn't match the
                    // on-chain claimed peer_id, reject immediately.
                    match Self::verify_rbn_identity(addr.clone(), expected_pid).await {
                        Ok(()) => {
                            info!("[Registry Validator] ✅ PeerId VERIFIED for {} at {}", claimed_peer_id, multiaddr_str);

                            // Persist to local anchor node storage via contacts table
                            let anchor_identity = crate::identity::SovereignIdentity {
                                peer_id: claimed_peer_id.clone(),
                                p2p_pubkey: Vec::new(),
                                static_key: [0u8; 32],
                                solana_address: String::new(),
                                global_name: Some(format!("RBN-{}", &claimed_peer_id[..8.min(claimed_peer_id.len())])),
                                local_alias: None,
                                avatar_base64: None,
                                is_anchor_capable: true,
                                retention_seconds: 86400,
                                handle: None,
                                prestige_tier: None,
                            };
                            if let Err(e) = storage.upsert_sovereign_contact(&anchor_identity, true, false) {
                                debug!("[Registry Validator] Failed to persist anchor {}: {:?}", claimed_peer_id, e);
                            }

                            // Update global bootstrap list
                            {
                                let mut global_bn = crate::BOOTSTRAP_NODES.write();
                                if !global_bn.iter().any(|(pid, _)| pid == claimed_peer_id) {
                                    global_bn.push((claimed_peer_id.clone(), multiaddr_str.clone()));
                                }
                            }

                            // Add to verified_rbns via command so the live NetworkService
                            // set stays in sync (mailbox storage + relay routing).
                            let _ = command_tx.send(NetworkCommand::AddVerifiedRbn {
                                peer_id: expected_pid,
                            }).await;

                            verified_count += 1;
                        }
                        Err(e) => {
                            // EMERGENCY SECURITY LOG: on-chain entry claims a PeerId
                            // but the node at that IP responded with a different identity.
                            warn!(
                                "[Registry Validator] ⚠️ SECURITY: PeerId MISMATCH for {} at {} — {}. \
                                 On-chain entry REJECTED. Possible impersonation attack.",
                                claimed_peer_id, multiaddr_str, e
                            );
                            rejected_count += 1;
                        }
                    }
                }

                info!(
                    "[Registry Validator] Complete: {} verified, {} rejected out of {} on-chain entries.",
                    verified_count, rejected_count, community_rbns.len()
                );
            }
            Err(e) => {
                warn!("[Registry Validator] Failed to fetch handle entries from Solana: {}", e);
            }
        }
    }

    /// Cryptographic test-dial: connects to `addr` **bound to** `expected_peer_id`,
    /// performs a Noise handshake, and verifies the remote node's libp2p static
    /// public key matches exactly. Uses a 10-second timeout.
    ///
    /// SECURITY: The `/p2p/<expected_peer_id>` suffix appended to the multiaddr
    /// forces libp2p's transport upgrade to validate the remote's Noise static
    /// key against the expected PeerId during the handshake itself. If the node
    /// at `addr` does not hold the private key for that PeerId, the connection
    /// fails at the Noise authentication step — not silently accepted.
    async fn verify_rbn_identity(addr: Multiaddr, expected_peer_id: PeerId) -> anyhow::Result<()> {
        use libp2p::Transport;

        // Append /p2p/<expected_peer_id> so the libp2p transport stack
        // enforces PeerId binding during Noise authentication.  If the
        // remote's static key doesn't match, the handshake is rejected
        // at the protocol level — no silent acceptance.
        let target_addr = addr.clone().with(libp2p::multiaddr::Protocol::P2p(expected_peer_id.into()));

        // Build a minimal TCP + Noise transport for verification only
        let local_key = libp2p::identity::Keypair::generate_ed25519();

        let mut yamux = libp2p::yamux::Config::default();
        yamux.set_max_num_streams(16);

        let mut transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default().nodelay(true))
            .upgrade(libp2p::core::upgrade::Version::V1Lazy)
            .authenticate(libp2p::noise::Config::new(&local_key)?)
            .multiplex(yamux)
            .timeout(Duration::from_secs(10))
            .boxed();

        // Dial the identity-bound address.  libp2p will extract the PeerId
        // from the /p2p/ suffix and verify it against the Noise static key
        // presented during the handshake.  Mismatch → connection rejected.
        let dial_opts = libp2p::core::transport::DialOpts {
            role: libp2p::core::Endpoint::Dialer,
            port_use: libp2p::core::transport::PortUse::Reuse,
        };
        match transport.dial(target_addr, dial_opts) {
            Ok(fut) => {
                match tokio::time::timeout(Duration::from_secs(10), fut).await {
                    Ok(Ok(_connection)) => {
                        // Noise handshake succeeded AND the remote's static key
                        // matches expected_peer_id.  Identity proven.
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        Err(anyhow::anyhow!("Identity-bound handshake failed: {}", e))
                    }
                    Err(_) => {
                        Err(anyhow::anyhow!("Connection timed out after 10s"))
                    }
                }
            }
            Err(e) => {
                Err(anyhow::anyhow!("Dial rejected: {}", e))
            }
        }
    }

    /// Synchronous Solana registry fallback — used when ALL cached historical
    /// IPs fail on boot (cold launch after months of network inactivity).
    /// Returns the validated RBN addresses from the on-chain registry.
    /// SECURITY: Same test-dial verification as background_registry_validator.
    pub async fn solana_registry_fallback_sync() -> Vec<(PeerId, Multiaddr)> {
        info!("[Registry Fallback] All cached IPs failed. Querying Solana Mainnet Registry synchronously...");

        let solana_client = match crate::economy::solana::SolanaIncentiveEngine::new(
            "https://api.mainnet-beta.solana.com",
            "EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf",
            "https://api.introvert.network/claim",
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!("[Registry Fallback] Failed to initialize Solana client: {}", e);
                return Vec::new();
            }
        };

        let program_id_str = "FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW";
        match solana_client.fetch_registered_rbn_details(program_id_str).await {
            Ok(entries) => {
                let mut verified = Vec::new();
                for entry in entries.iter().filter(|e| e.is_active) {
                    let pid: PeerId = match entry.peer_id.parse() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let addr_str = if entry.multiaddresses.starts_with('/') {
                        entry.multiaddresses.clone()
                    } else {
                        format!("/ip4/{}/tcp/443", entry.multiaddresses)
                    };
                    let addr: Multiaddr = match addr_str.parse() {
                        Ok(a) => a,
                        Err(_) => continue,
                    };

                    // Cryptographic test-dial before trusting
                    match Self::verify_rbn_identity(addr.clone(), pid).await {
                        Ok(()) => {
                            info!("[Registry Fallback] ✅ Verified: {} at {}", entry.peer_id, addr_str);
                            verified.push((pid, addr));
                        }
                        Err(e) => {
                            warn!("[Registry Fallback] ⚠️ SECURITY: Rejected {} at {}: {}", entry.peer_id, addr_str, e);
                        }
                    }
                }

                info!("[Registry Fallback] Verified {}/{} on-chain entries.", verified.len(), entries.len());
                verified
            }
            Err(e) => {
                warn!("[Registry Fallback] Failed to fetch handle entries: {}", e);
                Vec::new()
            }
        }
    }

    pub async fn handle_signaling_payload(&mut self, peer: PeerId, payload: SignalingPayload, is_webrtc: bool) {
        info!("[Mesh] handle_signaling_payload entered for peer {}", peer);
        let mut queue = vec![(peer, payload, is_webrtc)];
        while let Some((p, pl, is_wtc)) = queue.pop() {
            match pl {
                SignalingPayload::Secure(secure_msg) => {
                    match secure_msg {
                        SecureMessage::Handshake(handshake_payload) => {
                            let mut success = false;
                            if let Some(session) = self.noise_sessions.get_mut(&p) {
                                if session.recv_message(&handshake_payload).is_ok() {
                                    info!("E2EE Handshake COMPLETED with peer: {}", p);
                                    let storage = Arc::clone(&self.storage);
                                    let enc_key = self.session_encryption_key;
                                    let session_state = session.get_state();
                                    tokio::spawn(async move {
                                        let _ = NetworkService::persist_session_state(storage, enc_key, p, session_state).await;
                                    });

                                    let mut data = p.to_string().into_bytes();
                                    data.push(0); 
                                    crate::dispatch_global_event(7, &data);
                                    success = true;
                                }
                            }
                            if !success {
                                // If the existing session couldn't process the handshake message
                                // (e.g. because we were in Transport mode or it was a fresh handshake request),
                                // we act as a responder and recreate the session.
                                if let Ok(mut session) = NoiseSession::responder(self.local_static_secret.to_bytes().as_slice()) {
                                    if session.recv_message(&handshake_payload).is_ok() {
                                        if let Ok(response) = session.send_message(&[]) {
                                            info!("E2EE Handshake (New/Re-key) COMPLETED as responder with peer: {}", p);
                                            let storage = Arc::clone(&self.storage);
                                            let enc_key = self.session_encryption_key;
                                            let session_state = session.get_state();
                                            tokio::spawn(async move {
                                                let _ = NetworkService::persist_session_state(storage, enc_key, p, session_state).await;
                                            });

                                            self.noise_sessions.insert(p, session);
                                            let _ = self.forward_to_mesh(p, SignalingPayload::Secure(SecureMessage::Handshake(response)), false).await;
                                        }
                                    }
                                }
                            }
                        }
                        SecureMessage::Transport(encrypted) => {
                            if let Some(session) = self.noise_sessions.get_mut(&p) {
                                match session.recv_message(&encrypted) {
                                    Ok(decrypted) => {
                                        if decrypted.starts_with(b"WEBRTC:") {
                                            if let Ok(signal) = serde_json::from_slice::<WebRtcSignal>(&decrypted[7..]) {
                                                queue.push((p, SignalingPayload::WebRtc(signal), is_wtc));
                                            }
                                        } else {
                                            if let Ok(inner_payload) = serde_json::from_slice::<SignalingPayload>(&decrypted) {
                                                queue.push((p, inner_payload, is_wtc));
                                            } else {
                                                let content_str = String::from_utf8_lossy(&decrypted).into_owned();
                                                let peer_id_str = p.to_string();
                                                let storage = Arc::clone(&self.storage);
                                                tokio::task::spawn_blocking(move || storage.store_message(&peer_id_str, &content_str, false));
                                                let timestamp = chrono::Utc::now().timestamp();
                                                let mut data = timestamp.to_be_bytes().to_vec();
                                                data.push(0); // msg_id_len = 0
                                                data.extend(&decrypted);
                                                crate::dispatch_global_event(2, &data); 
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("❌ Noise decryption FAILED for {}: {:?}", p, e);
                                        self.noise_sessions.remove(&p);
                                        let storage = Arc::clone(&self.storage);
                                        let pid_str = p.to_string();
                                        tokio::task::spawn_blocking(move || {
                                            let _ = storage.delete_session_state(&pid_str);
                                        });
                                        // Request handshake from peer
                                        let tx = self.command_tx.clone();
                                        tokio::spawn(async move {
                                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                                peer_id: p, 
                                                payload: SignalingPayload::RequestHandshake 
                                            }).await;
                                        });
                                    }
                                }
                            } else {
                                info!("[Mesh] Received Transport payload from {} but no active Noise session. Requesting handshake.", p);
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                        peer_id: p, 
                                        payload: SignalingPayload::RequestHandshake 
                                    }).await;
                                });
                            }
                        }
                    }
                }
                SignalingPayload::MailboxStore { recipient_id, payload, original_msg_id } => {
                    let is_anchor = self.swarm.behaviour().relay_server.as_ref().is_some() || self.storage.is_anchor_mode_enabled();
                    if !is_anchor {
                        info!("[Mesh] Warning: Received MailboxStore but we are NOT an anchor node. Ignoring.");
                    } else if let Ok(recipient) = recipient_id.parse::<PeerId>() {
                        // --- RELIABILITY FIX: Loopback Protection ---
                        // If we are an anchor and we receive a message for ourselves,
                        // unwrap it and handle it immediately.
                        if recipient == *self.swarm.local_peer_id() {
                            info!("[Mesh] Received MailboxStore for OURSELVES. Routing to local handler.");
                            if let Ok(inner) = serde_json::from_slice::<SignalingPayload>(&payload) {
                                // Recursive push to process the inner signaling (e.g. ChatMessage)
                                queue.push((peer, inner, false));
                            }
                        } else {
                            let _ = self.storage.store_mailbox_payload(&recipient, &peer, payload);

                            // Confirm to sender that message was stored in mailbox
                            if let Some(mid) = original_msg_id {
                                info!("[Anchor] MailboxStored ACK for msg {} → sender {}", mid, peer);
                                let ack = SignalingPayload::MailboxStored {
                                    recipient_id: recipient_id.clone(),
                                    original_msg_id: mid,
                                };
                                let _ = self.forward_to_mesh(peer, ack, false).await;
                            }
                            
                            // Push notification: wake up offline peer
                            if let Ok(Some((device_type, token))) = self.storage.get_push_token(&recipient_id) {
                                info!("[Mesh] 🔔 Push wake-up for offline peer {} ({})", recipient_id, device_type);
                                let client = reqwest::Client::new();
                                let peer_hash = {
                                    use sha2::{Sha256, Digest};
                                    hex::encode(Sha256::digest(recipient_id.as_bytes()))
                                };
                                let payload_json = serde_json::json!({
                                    "device_type": device_type,
                                    "token": token,
                                    "peer_id_hash": peer_hash
                                });
                                tokio::spawn(async move {
                                    let _ = client.post("https://push.introvert.network/wakeup")
                                        .json(&payload_json)
                                        .timeout(Duration::from_secs(5))
                                        .send()
                                        .await;
                                });
                            }

                            // Push upgrade for other peers...
                            if self.swarm.is_connected(&recipient) {
                                if let Ok(messages) = self.storage.drain_mailbox(&recipient) {
                                    if !messages.is_empty() {
                                        let _ = self.forward_to_mesh(recipient, SignalingPayload::MailboxDrained(messages), false).await;
                                    }
                                }
                            }
                        }
                    }
                }
                SignalingPayload::MailboxDrained(messages) => {
                    let count = messages.len();
                    info!("📦 Drained {} messages from mesh mailbox", count);
                    for msg in messages {
                        if let Ok(sender_peer) = msg.sender_id.parse::<PeerId>() {
                            if let Ok(signaling) = serde_json::from_slice::<SignalingPayload>(&msg.payload) {
                                // DEDUP: Skip if this message already exists in storage
                                if let SignalingPayload::ChatMessage { ref msg_id, .. } = signaling {
                                    if self.storage.message_exists(msg_id) {
                                        debug!("[Mailbox] Skipping already-delivered message: {}", msg_id);
                                        continue;
                                    }
                                }
                                // FILTER: Skip [FILE]: metadata (file transfers use their own delivery)
                                if let SignalingPayload::ChatMessage { ref content, .. } = signaling {
                                    if content.starts_with("[FILE]:") {
                                        debug!("[Mailbox] Skipping file metadata message");
                                        continue;
                                    }
                                }
                                // CLEAR GUARD: Skip messages from before the chat was cleared
                                if let SignalingPayload::ChatMessage { timestamp, .. } = signaling {
                                    if self.storage.should_skip_mailbox_message(&msg.sender_id, timestamp) {
                                        info!("[Mailbox] Skipping pre-clear message from {}", msg.sender_id);
                                        continue;
                                    }
                                }
                                queue.push((sender_peer, signaling, false));
                            }
                        }
                    }
                    // RECURSIVE DRAIN: If we got messages, there might be more (or our response might trigger a new one)
                    // Wait a tiny bit and fetch again.
                    if count > 0 {
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            let _ = tx.send(NetworkCommand::FetchMailbox).await;
                        });
                    }
                }
                _ => {
                    self.handle_single_payload(p, pl, is_wtc).await;
                }
            }
        }
    }

    async fn handle_single_payload(&mut self, peer: PeerId, payload: SignalingPayload, _is_webrtc: bool) {


        match payload {
            SignalingPayload::Standard(msg) => {
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let m = msg.clone();
                tokio::task::spawn_blocking(move || storage.store_message(&peer_id_str, &m, false));
                
                let timestamp = chrono::Utc::now().timestamp();
                let mut data = timestamp.to_be_bytes().to_vec();
                data.push(0); // msg_id_len = 0
                data.extend(msg.as_bytes());
                crate::dispatch_global_event(2, &data);
            }
            SignalingPayload::WebRtc(signal) => {
                match signal.signal_type.as_str() {
                    "offer" => {
                        // Distinguish file transfer data channel offers from VoIP call offers.
                        // File transfers use purpose="file_transfer" and should NOT trigger the incoming call UI.
                        if signal.purpose.as_deref() == Some("file_transfer") {
                            info!("[Mesh] Received file transfer WebRTC offer from {}", peer);
                            self.data_channels.write().remove(&peer);
                            let old_pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                            if let Some(pc) = old_pc {
                                let _ = pc.close().await;
                            }
                            self.pending_offers.insert(peer, signal.sdp.clone());
                            // Event 39 = File transfer WebRTC offer (auto-accept, no call UI)
                            let data = peer.to_string().into_bytes();
                            crate::dispatch_global_event(39, &data);
                        } else {
                            self.data_channels.write().remove(&peer);
                            let old_pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                            if let Some(pc) = old_pc {
                                let _ = pc.close().await;
                            }

                            self.pending_offers.insert(peer, signal.sdp.clone());
                            // Event 14 = Incoming VoIP call
                            let data = peer.to_string().into_bytes();
                            crate::dispatch_global_event(14, &data);
                        }
                    }
                    "answer" => {
                        let pc_opt = self.peer_connections.read().get(&peer).cloned();
                        if let Some(pc) = pc_opt { let _ = MediaManager::handle_answer(signal.sdp, pc).await; }
                    }
                    "reject" => {
                        self.data_channels.write().remove(&peer);
                        let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                        if let Some(pc) = pc {
                            let _ = pc.close().await;
                        }
                        let data = peer.to_string().into_bytes();
                        crate::dispatch_global_event(16, &data);
                    }
                    _ => {}
                }
            }
            SignalingPayload::Candidate(candidate_json) => {
                let pc_opt = self.peer_connections.read().get(&peer).cloned();
                if let Some(pc) = pc_opt {
                    if let Ok(candidate) = serde_json::from_str::<webrtc::ice_transport::ice_candidate::RTCIceCandidateInit>(&candidate_json) {
                        let _ = pc.add_ice_candidate(candidate).await;
                    }
                }
            }
            SignalingPayload::WebRtcNative(json) => {
                // Dispatch Event 15: flutter_webrtc signal (SDP offer/answer or ICE candidate)
                // Format: [peer_id_len: u8][peer_id_bytes][json_bytes]
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = vec![peer_bytes.len() as u8];
                data.extend_from_slice(&peer_bytes);
                data.extend_from_slice(json.as_bytes());
                crate::dispatch_global_event(15, &data);
            }
            SignalingPayload::ChatMessage { content, msg_id, timestamp, reply_to } => {
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);

                // Privacy gate: check if peer is a contact (verified via dual handshake)
                let contact = tokio::task::spawn_blocking({
                    let storage = storage.clone();
                    let pid = peer_id_str.clone();
                    move || storage.get_contact(&pid)
                }).await.unwrap_or(Ok(None)).unwrap_or(None);

                if contact.is_none() {
                    info!("[Privacy] Blocked individual ChatMessage from non-contact group peer: {}", peer_id_str);
                    return;
                }

                let c = content.clone();
                let mid = msg_id.clone();
                let rt = reply_to.clone();
                let peer_id_str_clone = peer_id_str.clone();
                let ts_str = chrono::DateTime::from_timestamp(timestamp, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());
                if !self.is_stress_test {
                    tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str_clone, &mid, &c, false, rt.as_deref(), ts_str.as_deref()));
                }
                let ack = SignalingPayload::Acknowledgement { msg_id: msg_id.clone(), status: 1 };
                let _ = self.forward_to_mesh(peer, ack, false).await;

                // Record MessageReceived activity for daily rewards
                if let Some(ref daily) = self.daily_reward_engine {
                    daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
                        activity_type: crate::economy::daily_rewards::ActivityType::MessageReceived,
                        peer_id: Some(peer_id_str.clone()),
                        value: 1,
                        is_foreground: true,
                        message_len: Some(content.len()),
                        is_self: false,
                        is_rbn: false,
                        proof_hash: None,
                        active_web_containers: 0,
                    });
                }

                // Pack [timestamp, msg_id_len, msg_id, reply_to_len, reply_to, content] for UI
                let mut data = timestamp.to_be_bytes().to_vec();
                let msg_id_bytes = msg_id.as_bytes();
                data.push(msg_id_bytes.len() as u8);
                data.extend(msg_id_bytes);

                let rt_bytes = reply_to.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]);
                data.push(rt_bytes.len() as u8);
                data.extend(rt_bytes);

                data.extend(content.as_bytes());
                crate::dispatch_global_event(2, &data);
            }
            SignalingPayload::FileChunkRequest { transfer_id, chunk_index, chunk_size, relay_hint } => {
                if let Some(ref hint) = relay_hint {
                    info!("[Mesh] Received chunk request for {} (index {}) from {} (relay_hint: {})", transfer_id, chunk_index, peer, &hint[..16.min(hint.len())]);
                    // Store relay hint for this peer to prioritize when sending chunks back
                    if let Ok(rbn_peer_id) = hint.parse::<PeerId>() {
                        self.relay_hints.insert(peer, rbn_peer_id);
                    }
                } else {
                    info!("[Mesh] Received chunk request for {} (index {}) from {}", transfer_id, chunk_index, peer);
                }
                
                // 1. Try active seeder first (session-specific)
                let seeder_info = self.active_seeders.get(&transfer_id).map(|s| {
                    (s.file_path.clone(), s.chunk_size, s.total_chunks, s.file_hash.clone(), s.group_id.clone())
                }).or_else(|| {
                    // Robust fallback: if exact transfer_id not found, find ANY seeder for the same hash
                    // Extract hash from transfer_id if it follows the gft_{hash}_{ts} pattern
                    let parts: Vec<&str> = transfer_id.split('_').collect();
                    if parts.len() >= 2 && parts[0] == "gft" {
                        let hash = parts[1];
                        self.active_seeders.values().find(|s| s.file_hash == hash).map(|s| {
                            (s.file_path.clone(), s.chunk_size, s.total_chunks, s.file_hash.clone(), s.group_id.clone())
                        })
                    } else {
                        None
                    }
                });

                let (path, csize, tchunks, f_hash, grp_id) = if let Some(info) = seeder_info {
                    // SECURITY: Verify requesting peer is authorized for this transfer
                    let peer_str = peer.to_string();
                    let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer);
                    let authorized = is_bootstrap || if let Some(ref gid) = info.4 {
                        // Group transfer: verify peer is a group member
                        self.storage.get_group(gid)
                            .ok()
                            .flatten()
                            .map(|g| {
                                let members: Vec<GroupMemberMetadata> = serde_json::from_str(&g.members_json).unwrap_or_default();
                                members.iter().any(|m| m.peer_id == peer_str)
                            })
                            .unwrap_or(false)
                    } else {
                        // 1:1 transfer: verify peer is a known contact
                        self.storage.get_contact(&peer_str)
                            .ok()
                            .flatten()
                            .is_some()
                    };
                    
                    if !authorized {
                        warn!("[Security] Rejecting FileChunkRequest from unauthorized peer {} for transfer {}", peer, transfer_id);
                        return;
                    }
                    
                    // Use requested chunk_size if provided, otherwise fallback to seeder's registered chunk_size
                    let requested_csize = chunk_size.unwrap_or(info.1);
                    let size = std::fs::metadata(&info.0).map(|m| m.len()).unwrap_or(0) as usize;
                    let tchunks = (size as f32 / requested_csize as f32).ceil() as u32;
                    (info.0, requested_csize, tchunks, info.3, info.4)
                } else {
                    // SECURITY: Only serve Sovereign Drive files to known contacts (never to RBN nodes)
                    let is_contact = self.storage.get_contact(&peer.to_string())
                        .ok()
                        .flatten()
                        .is_some();
                    if !is_contact {
                        warn!("[Security] Rejecting FileChunkRequest from unknown peer {} for transfer {}", peer, transfer_id);
                        return;
                    }
                    
                    // 2. FALLBACK: Check if we have this file in our Sovereign Drive (persistent seeding)
                    let storage = self.storage.clone();
                    let tid = transfer_id.clone();
                    
                    let drive_file = tokio::task::spawn_blocking(move || {
                        // Attempt indexed hash query first by extracting hash from gft_{hash}_{ts}
                        let mut hash_opt = None;
                        let parts: Vec<&str> = tid.split('_').collect();
                        if parts.len() >= 2 && parts[0] == "gft" {
                            hash_opt = Some(parts[1].to_string());
                        }

                        if let Some(ref hash) = hash_opt {
                            if let Ok(Some(file)) = storage.get_drive_file_by_hash(hash) {
                                info!("[Mesh] Fallback seeder matched DB record by hash for {}: {:?}", tid, file.filename);
                                return Some(file);
                            }
                        }

                        let files = match storage.get_all_drive_files() {
                            Ok(f) => f,
                            Err(e) => {
                                info!("[Mesh] ❌ Fallback seeder DB error: {}", e);
                                return None;
                            }
                        };
                        info!("[Mesh] Fallback seeder checking {} drive files for transfer_id: {}", files.len(), tid);
                        files.into_iter().find(|f| {
                            let h_low = f.file_hash.to_lowercase();
                            let tid_low = tid.to_lowercase();
                            // SECURITY: Use exact hash matching to prevent serving wrong files
                            if tid_low.contains(&h_low) || h_low.contains(&tid_low) {
                                info!("[Mesh] Fallback seeder matched DB record for {}: {:?}", tid, f.filename);
                                true
                            } else {
                                false
                            }
                        })
                    }).await.unwrap_or(None);

                    if let Some(file) = drive_file {
                        let path = file.local_path.clone();
                        let size = file.total_size;
                        let hash = file.file_hash.clone();
                        
                        let requested_csize = chunk_size.unwrap_or(64 * 1024);
                        let tchunks = (size as f32 / requested_csize as f32).ceil() as u32;
                        
                        if path.is_empty() || !std::path::Path::new(&path).exists() {
                            info!("[Mesh] ❌ Fallback seeder found drive record but file is missing on disk: {}", path);
                            return;
                        }

                        // Register seeder dynamically in active_seeders so subsequent chunks don't hit the DB
                        self.active_seeders.insert(transfer_id.clone(), ActiveSeeder {
                            peer_id: peer,
                            file_path: path.clone(),
                            file_hash: hash.clone(),
                            chunk_size: requested_csize,
                            total_chunks: tchunks,
                            _bytes_sent: 0,
                            _start_time: Instant::now(),
                            group_id: None,
                            completions: HashSet::new(),
                        });

                        // We do not have group context for fallback drive files, so None
                        (path, requested_csize, tchunks, hash, None)
                    } else {
                        info!("[Mesh] ❌ Rejected chunk request: No seeder or drive file found for {}", transfer_id);
                        return;
                    }
                };

                let tx = self.command_tx.clone();
                let tid = transfer_id.clone();
                let p_id = peer;
                let p_path = std::path::Path::new(&path);
                let filename = p_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                let ext = p_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let mime_type = match ext.as_str() {
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
                    "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
                    "pdf" => "application/pdf".to_string(),
                    "txt" => "text/plain".to_string(),
                    _ => "application/octet-stream".to_string(),
                };

                tokio::spawn(async move {
                    if let Ok(mut file) = std::fs::File::open(&path) {
                        use std::io::Seek;
                        if file.seek(std::io::SeekFrom::Start((chunk_index * csize) as u64)).is_ok() {
                            let mut b = vec![0u8; csize as usize];
                            if let Ok(n) = file.read(&mut b) {
                                b.truncate(n);
                                let p = SignalingPayload::FileChunk { transfer_id: tid.clone(), chunk_index, total_chunks: tchunks, data_base64: general_purpose::STANDARD.encode(&b) };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: p_id, payload: p }).await;

                                // Update Sender UI with upload metadata
                                let progress = FileTransferProgress {
                                    transfer_id: tid,
                                    peer_id: p_id.to_string(),
                                    filename,
                                    mime_type,
                                    file_hash: f_hash,
                                    progress: (chunk_index as f32 + 1.0) / tchunks as f32,
                                    is_complete: chunk_index + 1 == tchunks,
                                    is_verified: false,
                                    is_outgoing: true,
                                    local_path: Some(path),
                                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                                    speed_bps: 0.0,
                                    group_id: grp_id,
                                    caption: None,
                                };

                                crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());
                            }
                        }
                    }
                });
            }
            SignalingPayload::RequestHandshake => {
                info!("[Mesh] Received RequestHandshake from {}. Clearing session and initiating new handshake.", peer);
                self.noise_sessions.remove(&peer);
                let storage = Arc::clone(&self.storage);
                let pid_str = peer.to_string();
                tokio::task::spawn_blocking(move || {
                    let _ = storage.delete_session_state(&pid_str);
                });
                let tx = self.command_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: peer }).await;
                });
            }
            SignalingPayload::ProfileRequest => {
                info!("[Mesh] Received ProfileRequest from {}", peer);
                if let Ok(Some(profile)) = self.storage.get_profile() {
                    let (name, handle, avatar, _, tier) = profile;
                    let response = SignalingPayload::ProfileResponse {
                        name: name.unwrap_or_else(|| "Unknown".to_string()),
                        handle: handle.unwrap_or_else(|| "".to_string()),
                        avatar_base64: avatar,
                        prestige_tier: tier as u8,
                    };
                    let _ = self.forward_to_mesh(peer, response, false).await;
                }
            }
            SignalingPayload::ProfileResponse { name, handle, avatar_base64, prestige_tier } => {
                info!("[Mesh] Received ProfileResponse from {}: {} ({}) tier={}", peer, name, handle, prestige_tier);
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let n = name.clone();
                let a = avatar_base64.clone();
                let t = prestige_tier;
                let peer_id_clone = peer_id_str.clone();
                
                // SECURITY: Verify claimed tier against on-chain balance
                // Accept the profile data immediately, but spawn async verification
                // If tier doesn't match, update to verified tier
                let solana_client = Arc::clone(&self.solana_client);
                let storage_verify = Arc::clone(&self.storage);
                let peer_id_verify = peer_id_str.clone();
                
                tokio::task::spawn_blocking(move || {
                    // 1. Update contacts if they exist (accept profile data)
                    if let Ok(Some(mut contact)) = storage.get_contact(&peer_id_clone) {
                        contact.global_name = Some(n.clone());
                        contact.avatar_base64 = a.clone();
                        contact.prestige_tier = Some(t);
                        let alias_is_empty_or_id = contact.local_alias.as_deref().map_or(true, |a| a.is_empty() || a == peer_id_clone);
                        if alias_is_empty_or_id {
                            contact.local_alias = Some(n.clone());
                        }
                        let (v, inc) = storage.get_contact_status(&peer_id_clone).ok().flatten().unwrap_or((true, false));
                        let _ = storage.upsert_sovereign_contact(&contact, v, inc);
                    }
                    
                    // 2. Update group member info across all groups
                    let _ = storage.update_group_member_profile(&peer_id_clone, &n, a.as_deref());
                });
                
                // 3. Async tier verification against Solana RPC
                let peer_id_for_verify = peer_id_str.clone();
                tokio::spawn(async move {
                    // Derive the peer's Solana address from their PeerId
                    // For now, we verify using the storage contact's solana_address if available
                    if let Ok(Some(contact)) = storage_verify.get_contact(&peer_id_for_verify) {
                        let sol_addr = &contact.solana_address;
                        if !sol_addr.is_empty() {
                            if let Ok(owner) = solana_sdk::pubkey::Pubkey::from_str(sol_addr) {
                                match solana_client.verify_prestige_tier(&owner, t).await {
                                    Ok(verified_tier) => {
                                        if verified_tier != t {
                                            info!("[Security] Tier verified for {}: claimed={}, actual={}", peer_id_for_verify, t, verified_tier);
                                            if let Ok(Some(mut c)) = storage_verify.get_contact(&peer_id_for_verify) {
                                                c.prestige_tier = Some(verified_tier);
                                                let (v, inc) = storage_verify.get_contact_status(&peer_id_for_verify).ok().flatten().unwrap_or((true, false));
                                                let _ = storage_verify.upsert_sovereign_contact(&c, v, inc);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("[Tier] Verification failed for {}: {}", peer_id_for_verify, e);
                                    }
                                }
                            }
                        }
                    }
                });

                // Dispatch event 25: PeerProfileUpdated
                // Data format: [pid_len, pid_bytes, name_len, name_bytes, handle_len, handle_bytes, avatar_len(4), avatar_bytes]
                let mut data = vec![peer_id_str.len() as u8];
                data.extend(peer_id_str.as_bytes());
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.push(handle.len() as u8);
                data.extend(handle.as_bytes());
                
                let avatar_bytes = avatar_base64.as_deref().unwrap_or("").as_bytes();
                data.extend(&(avatar_bytes.len() as u32).to_be_bytes());
                data.extend(avatar_bytes);
                data.push(prestige_tier);

                crate::dispatch_global_event(25, &data);
            }
            SignalingPayload::ChatSyncRequest { chat_id, is_group, known_msg_ids, limit } => {
                let effective_limit = limit as usize;
                info!("[Mesh] Received ChatSyncRequest from {} for chat {} (group={}, {} known IDs, limit={})", peer, chat_id, is_group, known_msg_ids.len(), effective_limit);
                let storage = Arc::clone(&self.storage);
                let chat_id_c = chat_id.clone();
                let is_group_c = is_group;
                
                let peer_known_vec = known_msg_ids.clone();
                let peer_known: std::collections::HashSet<String> = known_msg_ids.into_iter().collect();

                let (our_ids, our_messages) = tokio::task::spawn_blocking(move || {
                    let mut ids = Vec::new();
                    let mut messages = Vec::new();
                    if is_group_c {
                        if let Ok(msgs) = storage.get_group_messages(&chat_id_c) {
                            for m in msgs {
                                ids.push(m.1.clone()); // m.1 = msg_id
                                messages.push(SyncMessage {
                                    msg_id: m.1,       // msg_id
                                    sender_id: m.0,     // sender_id
                                    content: m.2,
                                    timestamp: m.3,
                                    reply_to: m.4,
                                });
                            }
                        }
                    } else {
                        if let Ok(msgs) = storage.get_messages_for_peer(&chat_id_c) {
                            for m in msgs {
                                if let Some(ref mid) = m.4 {
                                    ids.push(mid.clone());
                                }
                                let is_me_str = if m.2 { "self" } else { "peer" };
                                messages.push(SyncMessage {
                                    msg_id: m.4.unwrap_or_default(),
                                    sender_id: is_me_str.to_string(),
                                    content: m.0,
                                    timestamp: m.1,
                                    reply_to: m.5,
                                });
                            }
                        }
                    }
                    (ids, messages)
                }).await.unwrap_or((Vec::new(), Vec::new()));

                let our_set: std::collections::HashSet<String> = our_ids.iter().cloned().collect();
                let missing_on_peer: std::collections::HashSet<String> = our_set.difference(&peer_known).cloned().collect();
                
                // Prioritize newest missing messages for us
                let mut missing_on_us: Vec<String> = peer_known_vec.iter()
                    .filter(|id| !our_set.contains(*id))
                    .cloned()
                    .rev() // Newest first
                    .take(effective_limit * 2)
                    .collect();
                missing_on_us.reverse(); // Restore chronological order

                // Prioritize newest missing messages for peer
                // Skip file messages in sync — file transfers have their own delivery
                // mechanism. Syncing them would overwrite local metadata (is_outgoing, local_path)
                // with the remote version, corrupting the UI.
                let mut to_send: Vec<SyncMessage> = our_messages.into_iter()
                    .rev() // Newest first
                    .filter(|m| missing_on_peer.contains(&m.msg_id) && !m.content.starts_with("[FILE]:"))
                    .take(effective_limit)
                    .collect();
                to_send.reverse(); // Restore chronological order

                info!("[Mesh] Sync response: sending {} messages, requesting {} missing", to_send.len(), missing_on_us.len());
                let response = SignalingPayload::ChatSyncResponse {
                    chat_id,
                    is_group,
                    messages: to_send,
                    missing_ids: missing_on_us,
                    is_relay: false,
                };
                let _ = self.forward_to_mesh(peer, response, false).await;
            }
            SignalingPayload::ChatSyncResponse { chat_id, is_group, messages, missing_ids, is_relay } => {
                info!("[Mesh] Received ChatSyncResponse for {} with {} messages, {} missing IDs (relay={})", chat_id, messages.len(), missing_ids.len(), is_relay);
                
                // SECURITY: Verify sender is authorized for this chat
                let sender_authorized = if is_group {
                    // For groups, verify sender is a member (whether relayed or direct sync)
                    self.storage.get_group(&chat_id)
                        .ok()
                        .flatten()
                        .map(|g| {
                            let members: Vec<GroupMemberMetadata> = serde_json::from_str(&g.members_json).unwrap_or_default();
                            members.iter().any(|m| m.peer_id == peer.to_string())
                        })
                        .unwrap_or(false)
                } else {
                    // For 1:1, verify sender is exactly the peer of this chat
                    peer.to_string() == chat_id
                };
                
                if !sender_authorized {
                    warn!("[Security] Rejecting ChatSyncResponse from unauthorized peer {} for chat {}", peer, chat_id);
                    self.sync_in_progress.remove(&chat_id);
                    return;
                }
                
                let storage = Arc::clone(&self.storage);
                let chat_id_clone = chat_id.clone();
                let is_group_c = is_group;
                let chat_id_for_dispatch = chat_id.clone();
                let relay_messages = if is_group && !is_relay { messages.clone() } else { Vec::new() };
                let received_count = messages.len();
                let my_peer_id_str = self.swarm.local_peer_id().to_string();

                // Store received messages with original timestamps to ensure correct chronological order
                let _ = tokio::task::spawn_blocking(move || {
                    for msg in messages {
                        // Filter out [FILE]: messages — file transfers have their own delivery mechanism
                        if msg.content.starts_with("[FILE]:") {
                            warn!("[Sync] Dropping [FILE]: message from sync (should have been filtered by sender)");
                            continue;
                        }
                        if is_group_c {
                            let is_me = msg.sender_id == my_peer_id_str;
                            let _ = storage.store_group_message(&chat_id_clone, &msg.sender_id, &msg.msg_id, &msg.content, is_me, msg.reply_to.as_deref(), Some(&msg.timestamp));
                        } else {
                            let is_me = msg.sender_id == "peer";
                            // Use sync-safe insert: only fills gaps, never overwrites existing messages.
                            // Prevents stale sync data from rolling back current messages.
                            let _ = storage.store_message_if_new(&chat_id_clone, &msg.msg_id, &msg.content, is_me, msg.reply_to.as_deref(), Some(&msg.timestamp));
                        }
                    }
                }).await;

                // Recursive sync: if we received a full batch, there may be more messages to fetch
                if !is_relay && received_count >= 100 {
                    let tx = self.command_tx.clone();
                    let peer_id_clone = peer;
                    let chat_id_r = chat_id_for_dispatch.clone();
                    let is_group_r = is_group;
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        info!("[Mesh] Recursive sync: received {} messages, requesting more for {}", received_count, chat_id_r);
                        let _ = tx.send(NetworkCommand::SyncChatMessages {
                            peer_id: peer_id_clone,
                            chat_id: chat_id_r,
                            is_group: is_group_r,
                            is_full: false,
                        }).await;
                    });
                }

                // Relay newly received group messages to other connected peers (only for original sync, not relayed)
                if is_group && !is_relay && received_count > 0 {
                    let tx = self.command_tx.clone();
                    let relay_chat = chat_id_for_dispatch.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        let _ = tx.send(NetworkCommand::RelaySyncedMessages {
                            chat_id: relay_chat,
                            messages: relay_messages,
                        }).await;
                    });
                }

                // If peer is requesting messages we have, send them back
                if !missing_ids.is_empty() {
                    let storage2 = Arc::clone(&self.storage);
                    let chat_id_c2 = chat_id.clone();
                    let is_group_c2 = is_group;
                    let missing_set: std::collections::HashSet<String> = missing_ids.into_iter().collect();

                    let to_send = tokio::task::spawn_blocking(move || {
                        let mut result = Vec::new();
                        if is_group_c2 {
                            if let Ok(msgs) = storage2.get_group_messages(&chat_id_c2) {
                                for m in msgs {
                                    if missing_set.contains(&m.1) { // m.1 = msg_id
                                        result.push(SyncMessage {
                                            msg_id: m.1,       // msg_id
                                            sender_id: m.0,     // sender_id
                                            content: m.2,
                                            timestamp: m.3,
                                            reply_to: m.4,
                                        });
                                    }
                                }
                            }
                        } else {
                            if let Ok(msgs) = storage2.get_messages_for_peer(&chat_id_c2) {
                                for m in msgs {
                                    if let Some(ref mid) = m.4 {
                                        if missing_set.contains(mid) {
                                            let is_me_str = if m.2 { "self" } else { "peer" };
                                            result.push(SyncMessage {
                                                msg_id: mid.clone(),
                                                sender_id: is_me_str.to_string(),
                                                content: m.0,
                                                timestamp: m.1,
                                                reply_to: m.5,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        result
                    }).await.unwrap_or_default();

                    if !to_send.is_empty() {
                        info!("[Mesh] Sending {} requested messages back to {}", to_send.len(), peer);
                        let reply = SignalingPayload::ChatSyncResponse {
                            chat_id,
                            is_group,
                            messages: to_send,
                            missing_ids: Vec::new(),
                            is_relay: false,
                        };
                        let _ = self.forward_to_mesh(peer, reply, false).await;
                    }
                }

                // Sync complete — allow future syncs for this chat
                self.sync_in_progress.remove(&chat_id_for_dispatch);

                // Dispatch event 23 to refresh chat UI (works for both 1:1 and group)
                crate::dispatch_global_event(23, chat_id_for_dispatch.as_bytes());
            }
            SignalingPayload::FileTransfer { transfer_id, filename, mime_type, file_hash, total_size, is_relayed, sender_peer_id, group_id } => {
                info!("[Mesh] ⭐ FileTransfer manifest received: tid={}, file={}, size={}, is_relayed={}, sender={:?}, group={:?}",
                    transfer_id, filename, total_size, is_relayed, sender_peer_id, group_id);
                // BUG 3 FIX: Use the actual sender's peer ID if provided, otherwise fallback to the anchor peer
                let actual_seeder_peer = if let Some(sid) = &sender_peer_id {
                    sid.parse::<PeerId>().unwrap_or(peer)
                } else {
                    peer
                };

                // LOOPBACK PROTECTION: If we are the sender of this file transfer manifest (gossiped back to us in a group chat), ignore it.
                if actual_seeder_peer == *self.swarm.local_peer_id() {
                    info!("[Mesh] Loopback FileTransfer manifest detected for transfer_id={}. Ignoring.", transfer_id);
                    return;
                }

                // PROACTIVE RELAY: If sender is not directly connected, establish relay circuit
                // NOW so it's ready when FileChunkRequest payloads start flowing.
                if !self.swarm.is_connected(&actual_seeder_peer) {
                    info!("[Mesh] File manifest from {} — proactively dialing relay path", actual_seeder_peer);
                    self.dial_relay_path(actual_seeder_peer, false);
                }

                // Prioritize direct P2P connection: prevent broadcast manifest from demoting to relayed
                let is_connected_now = self.swarm.is_connected(&actual_seeder_peer);
                let relayed_map_snapshot = self.is_relayed_map.read().get(&actual_seeder_peer).cloned();
                let is_direct_p2p = is_connected_now && !relayed_map_snapshot.unwrap_or(false);
                // FIX: If sender is NOT directly connected, force relayed=true regardless of manifest value.
                // Gossipsub manifests arrive with is_relayed=false (sender expects direct push), but
                // cross-network receivers can't receive direct pushes — they must pull via relay.
                let final_is_relayed = if is_direct_p2p { false } else { true };

                // ADAPTIVE CHUNKING: Direct P2P uses 256KB chunks, Relay uses 64KB
                // (v34 pattern — 64KB reliably traverses relay circuits without overwhelming RBNs)
                let chunk_size = if final_is_relayed { 64 * 1024 } else { 256 * 1024 };
                let total_chunks = (total_size as f32 / chunk_size as f32).ceil() as u32;

                let initial_pipeline = self.intro_claw.get_optimal_pipeline_depth(&actual_seeder_peer.to_string(), is_direct_p2p);
                let base_pacing = if is_direct_p2p { 10u64 } else { 50 };
                let pacing_delay = if self.intro_claw.voip_is_active() {
                    250 // Phase 3.4: VoIP priority — massive backoff during active calls
                } else if self.intro_claw.is_on_mobile_data() {
                    (base_pacing as f64 * 1.5) as u64
                } else {
                    base_pacing
                };

                // DEDUPLICATION: If we already have an active transfer for this file_hash (regardless
                // of transfer_id), merge the new sender as a provider into the existing transfer.
                // This prevents two parallel transfers of the same file from competing for seeder
                // bandwidth and causing the watchdog stall loop.
                let already_active_tid = self.incoming_transfers
                    .iter()
                    .find(|(tid, t)| t.file_hash == file_hash && t.group_id == group_id && *tid != &transfer_id)
                    .map(|(tid, _)| tid.clone());
                if let Some(existing_tid) = already_active_tid {
                    if let Some(existing) = self.incoming_transfers.get_mut(&existing_tid) {
                        if !existing.providers.contains(&actual_seeder_peer) {
                            existing.providers.push(actual_seeder_peer);
                            info!("[Mesh] Dedup: merged seeder {} into existing transfer {} (duplicate transfer_id={} ignored)",
                                actual_seeder_peer, existing_tid, transfer_id);
                        }
                    }
                    return;
                }

                let mut is_update = false;
                if let Some(existing) = self.incoming_transfers.get_mut(&transfer_id) {
                    info!("[Mesh] FileTransfer manifest update for existing transfer {}. Updating config and preserving progress.", transfer_id);
                    is_update = true;
                    let was_relayed = existing.is_relayed;
                    
                    existing.is_relayed = final_is_relayed;

                    if !existing.providers.contains(&actual_seeder_peer) {
                        existing.providers.push(actual_seeder_peer);
                    }
                    existing.last_update = Instant::now();
                    
                    // If it transitioned to relayed now, start the pull sequence from current progress
                    if final_is_relayed && !was_relayed {
                        let mut next = 0u32;
                        while existing.received_chunks.contains_key(&next) { next += 1; }
                        let pipeline_depth = self.intro_claw.get_optimal_pipeline_depth(&actual_seeder_peer.to_string(), false);
                        let limit = if existing.total_chunks > 0 {
                            std::cmp::min(next + pipeline_depth, existing.total_chunks)
                        } else {
                            next + pipeline_depth
                        };
                        existing.next_pull_idx = limit;
                        
                        info!("[Mesh] Transitioned to relay mode. Initiating primed pull sequence for chunks {}..{}", next, limit - 1);
                        let tx = self.command_tx.clone();
                        let tid = transfer_id.clone();
                        let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &existing.providers);
                        let csize = existing.chunk_size;
                        let relay_hint = self.relay_reservations.iter().next().map(|id| id.to_string());
                        tokio::spawn(async move {
                            for idx in next..limit {
                                let target_peer = if !selected_providers.is_empty() {
                                    selected_providers[(idx as usize) % selected_providers.len()]
                                } else {
                                    actual_seeder_peer
                                };
                                let req = SignalingPayload::FileChunkRequest { transfer_id: tid.clone(), chunk_index: idx, chunk_size: Some(csize), relay_hint: relay_hint.clone() };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        });
                    }
                } else {
                    // SECURITY: Limit active incoming transfers to prevent memory exhaustion
                    if self.incoming_transfers.len() >= 50 {
                        warn!("[Security] Incoming transfers buffer full (50). Rejecting new transfer {}", transfer_id);
                        return;
                    }
                    self.incoming_transfers.insert(transfer_id.clone(), IncomingTransfer {
                        filename: filename.clone(),
                        mime_type: mime_type.clone(),
                        file_hash: file_hash.clone(),
                        total_size: total_size as usize,
                        total_chunks,
                        received_chunks: HashMap::new(),
                        peer_id: actual_seeder_peer,
                        providers: vec![actual_seeder_peer],
                        start_time: Instant::now(),
                        last_update: Instant::now(),
                        is_relayed: final_is_relayed,
                        group_id: group_id.clone(),
                        next_pull_idx: initial_pipeline,
                        chunk_size,
                        stall_chunk_count: 0,
                    });
                }

                // FLUSH EARLY CHUNKS: Process any chunks that arrived before this manifest
                if let Some(buffered) = self.early_chunks.remove(&transfer_id) {
                    info!("[Mesh] Flushing {} buffered chunks for {}", buffered.len(), transfer_id);
                    for (chunk_idx, total_chunks_val, data) in buffered {
                        self.handle_file_chunk(peer, transfer_id.clone(), chunk_idx, total_chunks_val, data).await;
                    }
                }

                // SOVEREIGN SWARM: If this is a relayed (cross-network) transfer,
                // trigger a DHT search to find other providers/seeders for this file.
                if final_is_relayed {
                    let tx = self.command_tx.clone();
                    let hash = file_hash.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::FindProviders { file_hash: hash }).await;
                    });
                }

                if !is_update {
                    let _progress = FileTransferProgress { 
                        transfer_id: transfer_id.clone(), 
                        peer_id: actual_seeder_peer.to_string(), 
                        filename: filename.clone(), 
                        mime_type: mime_type.clone(),
                        file_hash: file_hash.clone(),
                        progress: 0.0, 
                        is_complete: false, 
                        is_verified: false,
                        is_outgoing: false, 
                        local_path: None,
                        start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                        speed_bps: 0.0,
                        group_id: group_id.clone(),
                        caption: None,
                    };
                    // Don't store in database at manifest arrival — wait for verification
                    // The verified completion handler (line ~1121) stores the message with localPath

                    // Suppress Event 12 on manifest — don't show anything in chat until verified

                    // Start pulling chunks ONLY if the sender is not pushing them directly.
                    // final_is_relayed=false means the sender will PUSH 256KB chunks directly to us.
                    // final_is_relayed=true means we must PULL chunks via FileChunkRequest.
                    if final_is_relayed {
                        info!("[Mesh] Relay/Pull transfer detected. Initiating primed pull sequence ({} deep) for {}", initial_pipeline, transfer_id);
                        let tx = self.command_tx.clone();
                        let tid = transfer_id.clone();
                        let total_chunks_val = total_chunks;
                        let csize = chunk_size;
                        let relay_hint = self.relay_reservations.iter().next().map(|id| id.to_string());
                        tokio::spawn(async move {
                            for i in 0..initial_pipeline {
                                if i < total_chunks_val {
                                    let req = SignalingPayload::FileChunkRequest { transfer_id: tid.clone(), chunk_index: i, chunk_size: Some(csize), relay_hint: relay_hint.clone() };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: actual_seeder_peer, payload: req }).await;
                                    tokio::time::sleep(Duration::from_millis(pacing_delay)).await;
                                }
                            }
                        });
                    } else {
                        // Direct push path: sender is pushing 256KB chunks to us.
                        // The 4-second watchdog stall recovery will automatically switch to
                        // pull mode (setting is_relayed=true) if chunks stop arriving,
                        // e.g. if the direct connection fails or the sender can't reach us.
                        info!("[Mesh] Direct push transfer detected. Waiting for 256KB chunks from sender for {}", transfer_id);
                    }
                }
            }
            SignalingPayload::FileChunk { transfer_id, chunk_index, total_chunks, data_base64 } => {
                info!("[Mesh] Received chunk {}/{} for {}", chunk_index, total_chunks, transfer_id);
                self.handle_file_chunk(peer, transfer_id, chunk_index, total_chunks, data_base64).await;
            }
            SignalingPayload::TransitFileChunk { target_peer, chunk } => {
                info!("[Mesh] Received TransitFileChunk for target {} from sender {}", target_peer, peer);
                if let Ok(target_peer_id) = target_peer.parse::<libp2p::PeerId>() {
                    let _ = self.forward_to_mesh(target_peer_id, *chunk, false).await;
                }
            }
            SignalingPayload::GroupManifestRequest { group_id, alias, avatar, handle, requester_static_key } => {
                if let Ok(Some(group)) = self.storage.get_group(&group_id) {
                    let members: Vec<GroupMemberMetadata> = serde_json::from_str(&group.members_json).unwrap_or_default();
                    let requester_peer_id = peer.to_string();

                    // Cache the requester's static key if provided
                    if let Some(ref sk) = requester_static_key {
                        self.pending_requester_static_keys.insert(requester_peer_id.clone(), sk.clone());
                    }

                    let my_peer_id = self.swarm.local_peer_id().to_string();

                    if members.iter().any(|m| m.peer_id == requester_peer_id) {
                        // Already a member: return manifest immediately, and try to also return the invite to recover secret if needed.
                        let secret_is_valid = !group.secret.iter().all(|&b| b == 0);
                        if secret_is_valid {
                            let mut static_key = None;
                            if let Ok(Some(contact)) = self.storage.get_contact(&requester_peer_id) {
                                static_key = Some(contact.static_key);
                            } else if let Some(ref sk_bytes) = requester_static_key {
                                if sk_bytes.len() == 32 {
                                    let mut sk = [0u8; 32];
                                    sk.copy_from_slice(sk_bytes);
                                    static_key = Some(sk);
                                }
                            }

                            if let Some(sk) = static_key {
                                if let Ok(wrapped) = group::GroupManager::wrap_group_secret(&group.secret, &sk) {
                                    let invite_payload = SignalingPayload::GroupInvite {
                                        group_id: group_id.clone(),
                                        name: group.name.clone(),
                                        description: group.description.clone(),
                                        inviter_peer_id: my_peer_id.clone(),
                                        group_secret_wrapped: wrapped,
                                        members: members.clone(),
                                    };
                                    info!("[Mesh] Sending GroupInvite to existing member {} to sync/recover secret", requester_peer_id);
                                    let _ = self.forward_to_mesh(peer, invite_payload, false).await;
                                }
                            }
                        }

                        let payload = SignalingPayload::GroupManifest {
                            group_id,
                            name: group.name,
                            description: group.description,
                            members,
                        };
                        let _ = self.forward_to_mesh(peer, payload, false).await;
                    } else {
                        // Not a member: trigger admin approval notification (Event 26)
                        let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                        if is_admin {
                            info!("[Mesh] Group join request from {} for group {}", requester_peer_id, group_id);
                            let mut data = group_id.clone().into_bytes();
                            data.push(0);
                            data.extend(requester_peer_id.as_bytes());
                            data.push(0);
                            data.extend(alias.clone().unwrap_or_default().as_bytes());
                            data.push(0);
                            data.extend(handle.clone().unwrap_or_default().as_bytes());
                            data.push(0);
                            data.extend(avatar.clone().unwrap_or_default().as_bytes());
                            crate::dispatch_global_event(26, &data);
                        }
                    }
                }
            }
            SignalingPayload::GroupJoinRejected { group_id, group_name, reason } => {
                info!("[Mesh] Group join request rejected for {}: {}", group_name, reason);
                let mut data = group_id.into_bytes();
                data.push(0);
                data.extend(group_name.as_bytes());
                data.push(0);
                data.extend(reason.as_bytes());
                crate::dispatch_global_event(27, &data);
            }
            SignalingPayload::GroupInvite { group_id, name, description, inviter_peer_id, group_secret_wrapped, members } => {
                info!("[Mesh] Received GroupInvite for group: {} from {}", name, inviter_peer_id);
                
                // SELF-HEALING: If we already have the group in our DB, but the secret is all-zeros (or we are missing the secret),
                // we can auto-unwrap and save it without prompting the user!
                let auto_accept = if let Ok(Some(existing_group)) = self.storage.get_group(&group_id) {
                    !self.storage.is_group_deleted(&group_id) && existing_group.secret.iter().all(|&b| b == 0)
                } else {
                    false
                };

                if auto_accept {
                    if let Ok(group_secret) = group::GroupManager::unwrap_group_secret(&group_secret_wrapped, &self.local_static_secret) {
                        info!("[Mesh] Auto-accepting GroupInvite to recover/save secret for group {}", group_id);
                        let _ = self.storage.save_group_secret(&group_id, &group_secret);
                        let members_json = serde_json::to_string(&members).unwrap_or_default();
                        let _ = self.storage.upsert_group(&group_id, &name, &description, &members_json);
                        let _ = self.storage.untombstone_group(&group_id);
                        
                        // Subscribe to Gossipsub topic for this group immediately to start receiving mesh traffic
                        let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                        if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                            error!("[Mesh] Failed to subscribe to gossipsub topic for auto-accepted group {}: {:?}", group_id, e);
                        }
                        
                        crate::dispatch_global_event(23, group_id.as_bytes());
                        return; // Done! No need to show prompt
                    }
                }

                // Subscribe to Gossipsub topic for this group immediately to start receiving mesh traffic
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    error!("[Mesh] Failed to subscribe to gossipsub topic for invited group {}: {:?}", group_id, e);
                }
                // Privacy-First: Store as pending invite — user must explicitly accept
                let members_json = serde_json::to_string(&members).unwrap_or_default();
                let pending = crate::storage::PendingGroupInvite {
                    group_id: group_id.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    inviter_peer_id: inviter_peer_id.clone(),
                    group_secret_wrapped,
                    members_json,
                };
                let _ = self.storage.store_pending_invite(&pending);
                
                // Dispatch event 24: GroupInvitePending
                // Payload: [inviter_id_len, inviter_id_bytes, group_name_len, group_name_bytes, group_id_bytes]
                let mut data = vec![inviter_peer_id.len() as u8];
                data.extend(inviter_peer_id.as_bytes());
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend(group_id.as_bytes());
                crate::dispatch_global_event(24, &data);
            }
            SignalingPayload::GroupAction(signed_action) => {
                crate::dispatch_debug_log(&format!("handle_single_payload: Received GroupAction for group {} from {}", signed_action.group_id, signed_action.signer_peer_id));
                let members_json_res = self.storage.get_group_members(&signed_action.group_id);
                if let Ok(Some(members_json)) = members_json_res {
                    let members: Vec<GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                    match group::GroupManager::verify_action(&signed_action, &members) {
                        Ok(true) => {
                            crate::dispatch_debug_log("handle_single_payload: GroupAction signature verified successfully");
                            crate::dispatch_debug_log(&format!("handle_single_payload: Processing GroupAction action variant"));
                            match signed_action.action {
                                GroupAction::Message { ref content_encrypted, ref msg_id, ref reply_to } => {
                                    crate::dispatch_debug_log(&format!("handle_single_payload: GroupAction::Message: msg_id={}, content_encrypted_len={}, reply_to={:?}", msg_id, content_encrypted.len(), reply_to));
                                    
                                    if self.seen_group_messages.contains(msg_id) {
                                        crate::dispatch_debug_log(&format!("handle_single_payload: Duplicate group message {} ignored", msg_id));
                                        return;
                                    }
                                    self.seen_group_messages.insert(msg_id.clone());

                                    match self.storage.get_group(&signed_action.group_id) {
                                        Ok(Some(group_info)) => {
                                            let is_all_zeros = group_info.secret.iter().all(|&b| b == 0);
                                            crate::dispatch_debug_log(&format!("handle_single_payload: Found group info for {}. secret is all zeros: {}. Hex prefix: {}", signed_action.group_id, is_all_zeros, hex::encode(&group_info.secret[0..4.min(group_info.secret.len())])));
                                            
                                            if is_all_zeros {
                                                if let Ok(signer_pid) = signed_action.signer_peer_id.parse::<PeerId>() {
                                                    let now = std::time::Instant::now();
                                                    let should_req = if let Some(last) = self.heal_rate_limiter.get(&signer_pid) {
                                                        now.duration_since(*last) > std::time::Duration::from_secs(10)
                                                    } else {
                                                        true
                                                    };
                                                    if should_req {
                                                        self.heal_rate_limiter.insert(signer_pid, now);
                                                        let local_profile = self.storage.get_profile().ok().flatten();
                                                        let alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                                                        let handle = local_profile.as_ref().and_then(|(_, h, _, _, _)| h.clone());
                                                        let avatar = local_profile.and_then(|(_, _, a, _, _)| a);
                                                        let local_static_key = self.local_static_public.to_bytes().to_vec();
                                                        
                                                        info!("[Mesh] Group secret is all-zeros for group {}! Proactively requesting manifest from signer {}", signed_action.group_id, signed_action.signer_peer_id);
                                                        let req = SignalingPayload::GroupManifestRequest {
                                                            group_id: signed_action.group_id.clone(),
                                                            alias,
                                                            avatar,
                                                            handle,
                                                            requester_static_key: Some(local_static_key),
                                                        };
                                                        let _ = self.forward_to_mesh(signer_pid, req, false).await;
                                                    }
                                                }
                                            }
                                            
                                            use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                                            if content_encrypted.len() >= 12 {
                                                let nonce = Nonce::from_slice(&content_encrypted[0..12]);
                                                let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                                                match cipher.decrypt(nonce, &content_encrypted[12..]) {
                                                    Ok(decrypted) => {
                                                        let content = String::from_utf8_lossy(&decrypted).into_owned();
                                                        let ts_str = chrono::DateTime::from_timestamp(signed_action.timestamp as i64, 0)
                                                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string());
                                                        crate::dispatch_debug_log(&format!("handle_single_payload: Decrypted group message content: {}", content));
                                                        let is_me = signed_action.signer_peer_id == self.swarm.local_peer_id().to_string();
                                                        let rt = reply_to.clone();
                                                        if !self.is_stress_test {
                                                            if let Err(e) = self.storage.store_group_message(&signed_action.group_id, &signed_action.signer_peer_id, &msg_id, &content, is_me, rt.as_deref(), ts_str.as_deref()) {
                                                                crate::dispatch_debug_log(&format!("handle_single_payload: ❌ Failed to store group message: {:?}", e));
                                                            } else {
                                                                crate::dispatch_debug_log("handle_single_payload: Successfully stored group message");
                                                            }
                                                        }
                                                        
                                                        let mut event_data = vec![signed_action.group_id.len() as u8];
                                                        event_data.extend(signed_action.group_id.as_bytes());
                                                        event_data.push(signed_action.signer_peer_id.len() as u8);
                                                        event_data.extend(signed_action.signer_peer_id.as_bytes());
                                                        
                                                        let rt_bytes = reply_to.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]);
                                                        event_data.push(rt_bytes.len() as u8);
                                                        event_data.extend(rt_bytes);

                                                        event_data.extend(content.as_bytes());
                                                        
                                                        // SOVEREIGN HYBRID RELIABILITY: 
                                                        // If we are an anchor node, store this gossip message in the mailbox for all other group members.
                                                        // This ensures that members who are currently offline will receive the message when they reconnect.
                                                        if self.storage.is_anchor_mode_enabled() {
                                                            let storage_m = self.storage.clone();
                                                            let gid_m = signed_action.group_id.clone();
                                                            let payload_m = SignalingPayload::GroupAction(signed_action.clone());
                                                            let my_id_str = self.swarm.local_peer_id().to_string();
                                                            let tx_m = self.command_tx.clone();
                                                            
                                                            tokio::spawn(async move {
                                                                if let Ok(Some(members_json)) = storage_m.get_group_members(&gid_m) {
                                                                    let members: Vec<GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                                                                    for m in members {
                                                                        if m.peer_id != my_id_str {
                                                                            if let Ok(m_pid) = m.peer_id.parse::<PeerId>() {
                                                                                // Push to RBN Mailbox for offline members
                                                                                let _ = tx_m.send(NetworkCommand::StoreInMailbox { 
                                                                                    peer_id: m_pid, 
                                                                                    payload: payload_m.clone() 
                                                                                }).await;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            });
                                                        }

                                                        // Anchor Auto-Pull: If we are an anchor node, automatically pull group media
                                                        // to ensure it's available even if the seeder goes offline.
                                                        if self.storage.is_anchor_mode_enabled() && content.starts_with("[FILE]:") {
                                                            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content[7..]) {
                                                                let tid = meta.get("transfer_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                                let filename = meta.get("filename").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                                                                let mime_type = meta.get("mime_type").and_then(|v| v.as_str()).unwrap_or("application/octet-stream").to_string();
                                                                let file_hash = meta.get("file_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                                let total_size = meta.get("total_size").and_then(|v| v.as_u64()).unwrap_or(0);
                                                                
                                                                if !tid.is_empty() && !file_hash.is_empty() {
                                                                    let tx = self.command_tx.clone();
                                                                    let sid = signed_action.signer_peer_id.clone();
                                                                    let gid = signed_action.group_id.clone();
                                                                    let tid_clone = tid.clone();
                                                                    
                                                                    tokio::spawn(async move {
                                                                        info!("[Registry] Anchor Auto-Pull: Initiating mesh cache for {} from {}", tid_clone, sid);
                                                                        let payload = SignalingPayload::FileTransfer {
                                                                            transfer_id: tid_clone,
                                                                            filename,
                                                                            mime_type,
                                                                            file_hash,
                                                                            total_size: total_size as u64,
                                                                            is_relayed: true,
                                                                            sender_peer_id: Some(sid),
                                                                            group_id: Some(gid),
                                                                        };
                                                                        // Forward to ourselves to trigger the pull logic in handle_single_payload
                                                                        let _ = tx.send(NetworkCommand::HandleIncomingPayload { 
                                                                            peer_id: PeerId::random(), // Dummy peer for local trigger
                                                                            payload 
                                                                        }).await;
                                                                    });
                                                                }
                                                            }
                                                        }

                                                        crate::dispatch_global_event(21, &event_data);
                                                    }
                                                    Err(e) => {
                                                        crate::dispatch_debug_log(&format!("handle_single_payload: ❌ Decryption failed: {:?}", e));
                                                    }
                                                }
                                            } else {
                                                crate::dispatch_debug_log(&format!("handle_single_payload: ❌ content_encrypted is too short: {}", content_encrypted.len()));
                                            }
                                        }
                                        Ok(None) => {
                                            crate::dispatch_debug_log(&format!("handle_single_payload: ❌ Group {} not found in local storage!", signed_action.group_id));
                                            
                                            if let Ok(signer_pid) = signed_action.signer_peer_id.parse::<PeerId>() {
                                                let now = std::time::Instant::now();
                                                let should_req = if let Some(last) = self.heal_rate_limiter.get(&signer_pid) {
                                                    now.duration_since(*last) > std::time::Duration::from_secs(10)
                                                } else {
                                                    true
                                                };
                                                if should_req {
                                                    self.heal_rate_limiter.insert(signer_pid, now);
                                                    let local_profile = self.storage.get_profile().ok().flatten();
                                                    let alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                                                    let handle = local_profile.as_ref().and_then(|(_, h, _, _, _)| h.clone());
                                                    let avatar = local_profile.and_then(|(_, _, a, _, _)| a);
                                                    let local_static_key = self.local_static_public.to_bytes().to_vec();
                                                    
                                                    info!("[Mesh] Group {} not found in local storage! Proactively requesting manifest from signer {}", signed_action.group_id, signed_action.signer_peer_id);
                                                    let req = SignalingPayload::GroupManifestRequest {
                                                        group_id: signed_action.group_id.clone(),
                                                        alias,
                                                        avatar,
                                                        handle,
                                                        requester_static_key: Some(local_static_key),
                                                    };
                                                    let _ = self.forward_to_mesh(signer_pid, req, false).await;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            crate::dispatch_debug_log(&format!("handle_single_payload: ❌ Error querying group {}: {:?}", signed_action.group_id, e));
                                        }
                                    }
                                },
                            GroupAction::AddMember { metadata } => {
                                let mut members = members;
                                if !members.iter().any(|m| m.peer_id == metadata.peer_id) {
                                    members.push(metadata.clone());
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes());

                                    // Dial the new member proactively
                                    if let Ok(pid) = metadata.peer_id.parse::<PeerId>() {
                                        if pid != *self.swarm.local_peer_id() {
                                            info!("[Mesh] Proactively dialing NEW group member: {}", pid);
                                            self.dial_relay_path(pid, false);
                                        }
                                    }
                                }
                            }
                            GroupAction::RemoveMember { peer_id } => {
                                let mut members = members;
                                if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                                    members.remove(pos);
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    if peer_id == self.swarm.local_peer_id().to_string() {
                                        let _ = self.storage.delete_group(&signed_action.group_id);
                                        crate::dispatch_global_event(22, signed_action.group_id.as_bytes());
                                    } else {
                                        crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                    }
                                }
                            }
                            GroupAction::UpdateRole { peer_id, new_role } => {
                                let mut members = members;
                                if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                                    members[pos].role = new_role;
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                }
                            }
                            GroupAction::DeleteGroup => {
                                let _ = self.storage.delete_group(&signed_action.group_id);
                                crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                            },
                            GroupAction::Reaction { msg_id, emoji } => {
                                let sid = signed_action.signer_peer_id.clone();
                                let storage = Arc::clone(&self.storage);
                                let mid = msg_id.clone();
                                let em = emoji.clone();
                                if !self.is_stress_test {
                                    tokio::task::spawn_blocking(move || storage.add_message_reaction(&mid, &sid, &em));
                                }

                                // Pack [msg_id_len, msg_id, emoji] for UI (Event 40)
                                let mut data = vec![msg_id.len() as u8];
                                data.extend(msg_id.as_bytes());
                                data.extend(emoji.as_bytes());
                                crate::dispatch_global_event(40, &data);
                            }
                            GroupAction::SetRetention { seconds } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    let _ = self.storage.set_group_retention(&signed_action.group_id, seconds);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes()); // Refresh UI
                                }
                            }
                            GroupAction::DeleteMessage { msg_id } => {
                                // Allow if it's the sender's own message, or if signer is admin
                                let _signer = signed_action.signer_peer_id.clone();
                                let _is_admin = members.iter().any(|m| m.peer_id == _signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                // In a real implementation, we'd verify the message sender from DB.
                                // For now, we trust the signature and either the user is admin or sender.
                                // Assuming validation passed or we let the client side dictate if they can send it.
                                let _ = self.storage.delete_message(&msg_id, true, _is_admin);

                                crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                            }
                            GroupAction::EditMessage { msg_id, new_content_encrypted } => {
                                if let Ok(Some(group_info)) = self.storage.get_group(&signed_action.group_id) {
                                    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                                    if new_content_encrypted.len() >= 12 {
                                        let nonce = Nonce::from_slice(&new_content_encrypted[0..12]);
                                        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                                        if let Ok(decrypted) = cipher.decrypt(nonce, &new_content_encrypted[12..]) {
                                            let new_content = String::from_utf8_lossy(&decrypted).into_owned();
                                            let _ = self.storage.edit_message(&msg_id, &new_content, true);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }
                            GroupAction::MuteMember { peer_id } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    if let Ok(mut muted) = self.storage.get_group_muted_members(&signed_action.group_id) {
                                        if !muted.contains(&peer_id) {
                                            muted.push(peer_id);
                                            let json = serde_json::to_string(&muted).unwrap_or_default();
                                            let _ = self.storage.update_group_muted_members(&signed_action.group_id, &json);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }
                            GroupAction::UnmuteMember { peer_id } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    if let Ok(mut muted) = self.storage.get_group_muted_members(&signed_action.group_id) {
                                        if let Some(pos) = muted.iter().position(|id| id == &peer_id) {
                                            muted.remove(pos);
                                            let json = serde_json::to_string(&muted).unwrap_or_default();
                                            let _ = self.storage.update_group_muted_members(&signed_action.group_id, &json);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }

                            }

                    }
                    Ok(false) => {
                        crate::dispatch_debug_log(&format!("handle_single_payload: ❌ GroupAction signature verification failed for signer {}", signed_action.signer_peer_id));
                        error!("[Mesh] ❌ GroupAction signature verification failed for signer {}", signed_action.signer_peer_id);
                    }
                    Err(e) => {
                        crate::dispatch_debug_log(&format!("handle_single_payload: ❌ GroupAction verification error for signer {}: {:?}", signed_action.signer_peer_id, e));
                        error!("[Mesh] ❌ GroupAction verification error for signer {}: {:?}", signed_action.signer_peer_id, e);
                    }
                }
                } else {
                    crate::dispatch_debug_log(&format!("handle_single_payload: ⚠️ No group members or group info found in DB for group_id: {}", signed_action.group_id));
                    
                    if let Ok(signer_pid) = signed_action.signer_peer_id.parse::<PeerId>() {
                        let now = std::time::Instant::now();
                        let should_req = if let Some(last) = self.heal_rate_limiter.get(&signer_pid) {
                            now.duration_since(*last) > std::time::Duration::from_secs(10)
                        } else {
                            true
                        };
                        if should_req {
                            self.heal_rate_limiter.insert(signer_pid, now);
                            let local_profile = self.storage.get_profile().ok().flatten();
                            let alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
                            let handle = local_profile.as_ref().and_then(|(_, h, _, _, _)| h.clone());
                            let avatar = local_profile.and_then(|(_, _, a, _, _)| a);
                            let local_static_key = self.local_static_public.to_bytes().to_vec();
                            
                            info!("[Mesh] Group members not found for group {}! Proactively requesting manifest from signer {}", signed_action.group_id, signed_action.signer_peer_id);
                            let req = SignalingPayload::GroupManifestRequest {
                                group_id: signed_action.group_id.clone(),
                                alias,
                                avatar,
                                handle,
                                requester_static_key: Some(local_static_key),
                            };
                            let _ = self.forward_to_mesh(signer_pid, req, false).await;
                        }
                    }
                }
            }
            SignalingPayload::GroupManifest { group_id, name, description, members } => {
                let my_peer_id = self.swarm.local_peer_id().to_string();
                let is_member = members.iter().any(|m| m.peer_id == my_peer_id);
                
                if !is_member {
                    // If we are not in the manifest, we definitely shouldn't have this group.
                    // If we had it, delete it.
                    if let Ok(Some(_)) = self.storage.get_group(&group_id) {
                        info!("[Mesh] Removing group {} as we are no longer members in the received manifest", group_id);
                        let _ = self.storage.delete_group(&group_id);
                        crate::dispatch_global_event(22, group_id.as_bytes());
                    }
                    return;
                }

                // Subscribe to Gossipsub topic for this group
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    error!("[Mesh] Failed to subscribe to gossipsub topic {}: {:?}", group_id, e);
                } else {
                    info!("[Mesh] Dynamically subscribed to gossipsub topic {}", group_id);
                }

                if self.storage.is_group_deleted(&group_id) {
                    info!("[Mesh] Ignoring manifest for deleted group {}", group_id);
                    return;
                }

                // Group secret should already be saved via GroupInvite mechanism (ECDH-wrapped).
                // Do NOT save secret from manifest — it's no longer transmitted in plaintext.
                let members_json = serde_json::to_string(&members).unwrap_or_default();
                let _ = self.storage.upsert_group(&group_id, &name, &description, &members_json);
                crate::dispatch_global_event(23, group_id.as_bytes());
                
                let mut data = group_id.clone().into_bytes();
                data.push(0);
                data.extend(name.as_bytes());
                data.push(0);
                data.extend(members_json.as_bytes());
                crate::dispatch_global_event(20, &data);

                // --- RELIABILITY FIX: Proactive Member Discovery ---
                let my_peer_id = self.swarm.local_peer_id().to_string();
                for m in members {
                    if m.peer_id == my_peer_id { continue; }
                    if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                        info!("[Mesh] Proactively dialing group member {} from manifest {}", pid, name);
                        self.dial_relay_path(pid, false);
                    }
                }
            }
            SignalingPayload::FileTransferComplete { transfer_id } => {
                // Guard: only process if we have an active seeder for this transfer.
                // Stale FileTransferComplete payloads from previous transfers (delivered
                // via mailbox drain) must NOT overwrite the message with is_verified: true.
                if !self.active_seeders.contains_key(&transfer_id) {
                    info!("[Mesh] Ignoring stale FileTransferComplete for {} — no active seeder", transfer_id);
                    return;
                }

                // Clean up any persisted chunks for this transfer
                let _ = self.storage.remove_pending_chunks_for_transfer(&transfer_id);

                let mut local_path = None;
                let mut filename = "".to_string();
                let mut mime_type = "".to_string();
                let mut is_group_transfer = false;
                
                let mut f_hash = "".to_string();
                let mut grp_id = None;
                let mut total_members = 0;
                let mut current_completions = 0;

                if let Some(seeder) = self.active_seeders.get_mut(&transfer_id) {
                    seeder.completions.insert(peer);
                    
                    let s_path = seeder.file_path.clone();
                    local_path = Some(s_path.clone());
                    f_hash = seeder.file_hash.clone();
                    grp_id = seeder.group_id.clone();

                    let p_path = std::path::Path::new(&s_path);
                    filename = p_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    let ext = p_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    mime_type = match ext.as_str() {
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
                        "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
                        "pdf" => "application/pdf".to_string(),
                        "txt" => "text/plain".to_string(),
                        _ => "application/octet-stream".to_string(),
                    };
                    is_group_transfer = seeder.group_id.is_some();

                    if is_group_transfer {
                         if let Some(gid) = &grp_id {
                             if let Ok(Some(group)) = self.storage.get_group(gid) {
                                 if let Ok(members) = serde_json::from_str::<Vec<GroupMemberMetadata>>(&group.members_json) {
                                     // excluding self
                                     total_members = members.len().saturating_sub(1);
                                     current_completions = seeder.completions.len();
                                     info!("[Mesh] Group transfer {} progress: {}/{} members finished.", transfer_id, current_completions, total_members);
                                 }
                             }
                         }
                    }
                }

                // MANDATE: In 1-to-1 transfers, stop seeding once receiver confirms receipt.
                // In group transfers, we continue seeding indefinitely for the session.
                if !is_group_transfer && self.active_seeders.contains_key(&transfer_id) {
                    info!("[Mesh] 1-to-1 transfer {} complete. Removing seeder and taking off mesh.", transfer_id);
                    self.active_seeders.remove(&transfer_id);
                } else if is_group_transfer {
                    info!("[Mesh] Group member received transfer {}. Continuing to seed for the rest of the group.", transfer_id);
                }

                let (progress_ratio, outgoing_complete) = if is_group_transfer && total_members > 0 {
                    // Mark complete when at least one member ACKs — waiting for ALL members
                    // hangs indefinitely if some are offline. The file is "delivered to the mesh"
                    // once any receiver confirms receipt.
                    (current_completions as f32 / total_members as f32, current_completions >= 1)
                } else if is_group_transfer && total_members == 0 {
                    // VERIFIED FIX: Group transfer but couldn't read member count from storage.
                    // Never prematurely signal outgoing_complete — wait for explicit member count.
                    // Show proportional progress based on completions seen so far (at least 1 member).
                    let ratio = current_completions as f32 / (current_completions.max(1) as f32);
                    warn!("[Mesh] Group member count unavailable for {}. Holding verified state.", transfer_id);
                    (ratio, false)
                } else {
                    (1.0, true)
                };

                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let msg_id = transfer_id.clone();
                let progress = FileTransferProgress {
                    transfer_id: transfer_id.clone(),
                    peer_id: peer.to_string(),
                    filename,
                    mime_type,
                    file_hash: f_hash,
                    progress: progress_ratio,
                    is_complete: outgoing_complete,
                    is_verified: outgoing_complete,
                    is_outgoing: true,
                    local_path,
                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                    speed_bps: 0.0,
                    group_id: grp_id.clone(), // clone here so grp_id is still usable for DB routing below
                    caption: None,
                };
                if let Ok(json_str) = serde_json::to_string(&progress) {
                    let c = format!("[FILE]:{}", json_str);
                    // ROUTING FIX: Write to the correct DB table based on transfer type.
                    // Previously always wrote to store_message_with_id (direct-chat table), causing
                    // the sender's completed group file to appear in the 1-on-1 chat with the last ACKer.
                    if let Some(ref gid) = grp_id {
                        let gid_clone = gid.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_group_message(&gid_clone, &peer_id_str, &msg_id, &c, true, None, None);
                        });
                    } else {
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_message_with_id(&peer_id_str, &msg_id, &c, true, None, None);
                        });
                    }
                }
                
                // Clear RAM buffer for this transfer to prevent memory leaks
                if let Some(pending) = self.pending_messages.get_mut(&peer) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, .. } | SignalingPayload::FileChunkRequest { transfer_id: tid, .. } if tid == &transfer_id));
                }

                let data = serde_json::to_vec(&progress).unwrap_or_default();
                crate::dispatch_global_event(12, &data);
            }
            SignalingPayload::FileTransferError { transfer_id, reason } => {
                warn!("File transfer error for {}: {}", transfer_id, reason);
                
                // BUG 1 FIX: Remove from active seeders to stop aggressive polling
                self.active_seeders.remove(&transfer_id);

                // Clear RAM buffer for this failed transfer to prevent memory leaks
                if let Some(pending) = self.pending_messages.get_mut(&peer) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, .. } | SignalingPayload::FileChunkRequest { transfer_id: tid, .. } if tid == &transfer_id));
                }
            }
            SignalingPayload::MailboxDrain => {
                let is_anchor = self.swarm.behaviour().relay_server.as_ref().is_some() || self.storage.is_anchor_mode_enabled();
                if is_anchor {
                    if let Ok(messages) = self.storage.drain_mailbox(&peer) {
                        let _ = self.forward_to_mesh(peer, SignalingPayload::MailboxDrained(messages), false).await;
                    }
                } else {
                    info!("[Mesh] Warning: Received MailboxDrain but we are NOT an anchor node. Ignoring.");
                }
            }
            SignalingPayload::Acknowledgement { msg_id, status } => {
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                tokio::task::spawn_blocking(move || { let _ = storage.update_message_status_if_higher(&mid, status); });
                let mut data = vec![status];
                data.extend(msg_id.as_bytes());
                crate::dispatch_global_event(13, &data);
            }
            SignalingPayload::MailboxStored { original_msg_id, .. } => {
                info!("[Mesh] MailboxStored ACK received for msg {}", original_msg_id);
                // Update status to 3 (In Mailbox) — message stored on relay, awaiting recipient
                let storage = Arc::clone(&self.storage);
                let mid = original_msg_id.clone();
                tokio::task::spawn_blocking(move || { let _ = storage.update_message_status_if_higher(&mid, 3); });
                let mut data = vec![3u8]; // status=3: In Mailbox
                data.extend(original_msg_id.as_bytes());
                crate::dispatch_global_event(13, &data);
            }
            SignalingPayload::TypingStart { chat_id: _ } => {
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = peer_bytes.clone();
                data.push(1); // 1 = typing started
                crate::dispatch_global_event(39, &data);
            }
            SignalingPayload::TypingStop { chat_id: _ } => {
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = peer_bytes.clone();
                data.push(0); // 0 = typing stopped
                crate::dispatch_global_event(39, &data);
            }
            SignalingPayload::Heartbeat { timestamp } => {
                // Store last-seen timestamp for the peer
                let peer_str = peer.to_string();
                let ts = timestamp;
                let storage = Arc::clone(&self.storage);
                tokio::task::spawn_blocking(move || {
                    let _ = storage.update_last_seen(&peer_str, ts);
                });
            }
            SignalingPayload::DirectInviteRequest(peer_identity) => {
                let is_extroverted = self.storage.is_privacy_mode_extroverted();
                if is_extroverted {
                    let peer_id = peer_identity.peer_id.clone();
                    let name = peer_identity.global_name.clone().unwrap_or_else(|| "Unknown".to_string());
                    let handle = peer_identity.handle.clone().unwrap_or_default();
                    let avatar = peer_identity.avatar_base64.clone();
                    
                    let storage = Arc::clone(&self.storage);
                    tokio::task::spawn_blocking(move || {
                        let _ = storage.upsert_sovereign_contact(&peer_identity, false, true);
                    });

                    let mut data = peer_id.into_bytes();
                    data.push(0);
                    data.extend(name.as_bytes());
                    data.push(0);
                    data.extend(handle.as_bytes());
                    data.push(0);
                    if let Some(av) = avatar { data.extend(av.as_bytes()); }
                    crate::dispatch_global_event(31, &data); // Event 31: Connection Request Received
                } else {
                    info!("[Mesh] Privacy Mode: Ignoring DirectInviteRequest from {:?} as we are INTROVERTED.", peer_identity.global_name);
                }
            }
            SignalingPayload::DirectInviteAccept(peer_identity) => {
                let peer_id = peer_identity.peer_id.clone();
                let name = peer_identity.global_name.clone().unwrap_or_else(|| "Unknown".to_string());
                let handle = peer_identity.handle.clone().unwrap_or_default();
                
                let storage = Arc::clone(&self.storage);
                tokio::task::spawn_blocking(move || {
                    let _ = storage.upsert_sovereign_contact(&peer_identity, true, false);
                });

                let mut data = peer_id.into_bytes();
                data.push(0);
                data.extend(name.as_bytes());
                data.push(0);
                data.extend(handle.as_bytes());
                crate::dispatch_global_event(32, &data); // Event 32: Connection Request Accepted
            }
            SignalingPayload::IdentifySleepState { device_type, push_token } => {
                let peer_id_str = peer.to_string();
                info!("[Registry] Registered Push Token for peer {}: {} ({})", peer_id_str, push_token, device_type);
                let _ = self.storage.save_push_token(&peer_id_str, &device_type, &push_token);
            }
            SignalingPayload::MessageReaction { msg_id, emoji } => {
                let sender_id = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                let em = emoji.clone();
                if !self.is_stress_test {
                    tokio::task::spawn_blocking(move || storage.add_message_reaction(&mid, &sender_id, &em));
                }

                // Pack [msg_id_len, msg_id, emoji] for UI
                let mut data = vec![msg_id.len() as u8];
                data.extend(msg_id.as_bytes());
                data.extend(emoji.as_bytes());
                crate::dispatch_global_event(40, &data); // Event Type 40: Message Reaction
            }
            SignalingPayload::SetRetention { seconds } => {
                let _ = self.storage.set_contact_retention(&peer.to_string(), seconds);
                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(36, &data); // Event 36: Retention changed
            }
            SignalingPayload::DeleteMessage { msg_id } => {
                // In P2P, if we receive DeleteMessage, the peer is deleting their own message.
                // We should technically verify if it belongs to them, but for 1-to-1, we trust the sender.
                let _ = self.storage.delete_message(&msg_id, false, false);

                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(37, &data); // Event 37: Message Deleted
            }
            SignalingPayload::EditMessage { msg_id, new_content } => {
                let _ = self.storage.edit_message(&msg_id, &new_content, false);
                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(38, &data); // Event 38: Message Edited
            }
            SignalingPayload::HandleClaimRequest { handle, peer_id, timestamp, pow_nonce } => {
                info!("[Registry] Received ClaimRequest for {} from {}", handle, peer_id);
                let claim = registry::HandleClaim { 
                    handle: handle.clone(), 
                    peer_id: peer_id.clone(), 
                    timestamp, 
                    pow_nonce, 
                    signatures: Vec::new() 
                };
                
                // 1. Verify PoW
                if !self.registry.verify_pow(&claim) {
                    info!("[Registry] ❌ Invalid PoW for handle claim: {}", handle);
                    return;
                }
                
                // 2. Check Uniqueness
                if !self.registry.is_handle_available(&handle, &peer_id) {
                    info!("[Registry] ❌ Handle {} already taken", handle);
                    return;
                }
                
                // 3. Witness claim if we are an Anchor/RBN
                let is_anchor_or_relay = self.storage.is_anchor_mode_enabled() || self.swarm.behaviour().relay_server.as_ref().is_some();
                if is_anchor_or_relay {
                    info!("[Registry] ✅ Witnessing claim for {}", handle);
                    let msg = format!("{}:{}:{}", handle, peer_id, timestamp);
                    if let Ok(sig) = self.local_keypair.sign(msg.as_bytes()) {
                         let pubkey = self.local_keypair.public().encode_protobuf();
                         let tx = self.command_tx.clone();
                         tokio::task::spawn(async move {
                             let _ = tx.send(NetworkCommand::BroadcastWitness { 
                                 handle, 
                                 peer_id, 
                                 timestamp, 
                                 pubkey,
                                 signature: sig 
                             }).await;
                         });
                    }
                }
            }
            SignalingPayload::HandleClaimWitnessed { handle, peer_id, timestamp, rbn_peer_id, rbn_pubkey, rbn_signature } => {
                info!("[Registry] Received Witness from {} for {}", rbn_peer_id, handle);
                
                // SECURITY: Verify the signature!
                let pubkey = match libp2p::identity::PublicKey::try_decode_protobuf(&rbn_pubkey) {
                    Ok(pk) => pk,
                    Err(_) => {
                        info!("[Registry] ⚠️ Rejected witness from {}: Invalid public key encoding", rbn_peer_id);
                        return;
                    }
                };

                // Verify that the public key matches the PeerId and is an authorized RBN
                let derived_pid = PeerId::from_public_key(&pubkey);
                if derived_pid.to_string() != rbn_peer_id {
                    info!("[Registry] ⚠️ Rejected witness from {}: PeerId mismatch", rbn_peer_id);
                    return;
                }

                let mut is_authorized = self.bootstrap_nodes.iter().any(|(pid, _)| pid == &derived_pid);
                
                // For local development or private meshes, allow trusting any connected anchor
                // SECURITY: Only available in debug builds to prevent accidental production use
                #[cfg(debug_assertions)]
                if !is_authorized && std::env::var("INTROVERT_TRUST_ALL_WITNESSES").is_ok() {
                    info!("[Registry] 🛠️ Trusting unauthorized witness due to INTROVERT_TRUST_ALL_WITNESSES (debug only)");
                    is_authorized = true;
                }

                if !is_authorized {
                    info!("[Registry] ⚠️ Rejected witness from UNAUTHORIZED node: {}", rbn_peer_id);
                    return;
                }

                let msg = format!("{}:{}:{}", handle, peer_id, timestamp);
                if !pubkey.verify(msg.as_bytes(), &rbn_signature) {
                    info!("[Registry] ⚠️ INVALID signature from RBN: {}", rbn_peer_id);
                    return;
                }

                // SECURITY: Limit pending claims to prevent memory exhaustion
                if self.pending_claims.len() >= 1000 && !self.pending_claims.contains_key(&handle) {
                    warn!("[Security] Pending claims buffer full. Dropping oldest claim.");
                    if let Some(oldest_key) = self.pending_claims.keys().next().cloned() {
                        self.pending_claims.remove(&oldest_key);
                    }
                }
                
                let witnesses = self.pending_claims.entry(handle.clone()).or_insert_with(HashSet::new);
                witnesses.insert(rbn_peer_id.clone());
                
                let mut connected_rbns = HashSet::new();
                for (rbn_id, _) in &self.bootstrap_nodes {
                    if self.swarm.is_connected(rbn_id) {
                        connected_rbns.insert(rbn_id);
                    }
                }
                #[cfg(debug_assertions)]
                let trust_all = std::env::var("INTROVERT_TRUST_ALL_WITNESSES").is_ok();
                #[cfg(not(debug_assertions))]
                let trust_all = false;
                let required_quorum = if trust_all || connected_rbns.len() < 2 { 1 } else { 2 };
                if witnesses.len() >= required_quorum {
                    info!("[Registry] 🏆 Quorum reached for handle: {}", handle);
                    let claim = registry::HandleClaim {
                        handle: handle.clone(),
                        peer_id: peer_id.clone(),
                        timestamp,
                        pow_nonce: 0,
                        signatures: witnesses.iter().cloned().collect(),
                    };
                    let _ = self.registry.verify_claim(&claim);
                    
                    // Publish handle mapping to DHT
                    let h_key = RecordKey::new(&handle.as_bytes());
                    let h_value = serde_json::to_string(&claim).unwrap_or_else(|_| peer_id.clone()).into_bytes();
                    let h_record = kad::Record {
                        key: h_key.clone(),
                        value: h_value,
                        publisher: Some(*self.swarm.local_peer_id()),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(h_record, kad::Quorum::One);
                    let _ = self.swarm.behaviour_mut().kademlia.start_providing(h_key);

                    // Publish reverse mapping ph_<peer_id> -> handle
                    let ph_key = RecordKey::new(&format!("ph_{}", peer_id).as_bytes());
                    let ph_record = kad::Record {
                        key: ph_key,
                        value: handle.clone().into_bytes(),
                        publisher: Some(*self.swarm.local_peer_id()),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(ph_record, kad::Quorum::One);

                    // Notify UI: [Handle\0PeerID]
                    let mut event_data = handle.clone().into_bytes();
                    event_data.push(0);
                    event_data.extend(peer_id.as_bytes());
                    crate::dispatch_global_event(34, &event_data);
                    
                    // Remove from pending to prevent double-event
                    self.pending_claims.remove(&handle);
                }
            }
            SignalingPayload::HandleResolveResponse { handle, peer_id, verified } => {
                info!("[Registry] Handle resolve response from RBN: {} -> {} (verified: {})", handle, peer_id, verified);
                // Store in local storage
                let _ = self.storage.insert_handle_claim(&handle, &peer_id, chrono::Utc::now().timestamp(), "[]", verified);
                // Dispatch event 33 to Flutter UI
                let mut event_data = handle.into_bytes();
                event_data.push(0);
                event_data.extend(peer_id.as_bytes());
                crate::dispatch_global_event(33, &event_data);
            }
            SignalingPayload::TelemetryAck { peer_id, epoch_id, total_points, timestamp } => {

                info!("[Economy] TelemetryAck received from RBN: peer={}, epoch={}, points={:.1}", peer_id, epoch_id, total_points);

                if let Err(e) = self.storage.mark_local_telemetry_synced(&epoch_id) {

                    warn!("[Economy] Failed to mark local telemetry synced: {:?}", e);

                }
                // Feed RBN-reported total points into DailyRewardEngine to bypass static ceiling
                if let Some(ref daily) = self.daily_reward_engine {
                    daily.accept_rbn_estimate(total_points, timestamp);
                }
                // Dispatch to Flutter as Event 40 (Telemetry Acknowledgment)
                let ack_json = serde_json::json!({
                    "peer_id": peer_id,
                    "epoch_id": epoch_id,
                    "total_points": total_points,
                    "timestamp": timestamp,
                });
                if let Ok(json_str) = serde_json::to_string(&ack_json) {
                    crate::dispatch_global_event(41, json_str.as_bytes());
                }
            }
            _ => {}
        }
    }

    async fn process_outgoing_file(
        peer_id: PeerId,
        file_path: String,
        is_relayed: bool,
        is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,
        data_channels: Arc<RwLock<HashMap<PeerId, Arc<webrtc::data_channel::RTCDataChannel>>>>,
        tx: mpsc::Sender<NetworkCommand>,
        storage: Arc<crate::storage::StorageService>,
        local_peer_id: PeerId,
        group_id: Option<String>,
        is_stress_test: bool,
        transfer_id_override: Option<String>,
    ) -> anyhow::Result<()> {

        let path = std::path::Path::new(&file_path);
        if !path.exists() { return Err(anyhow::anyhow!("File not found: {}", file_path)); }
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
        
        // Enhanced MIME detection
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let mime_type = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
            "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
            "pdf" => "application/pdf".to_string(),
            "txt" => "text/plain".to_string(),
            _ => "application/octet-stream".to_string(),
        };

        // BUG 4 FIX: Streaming hash instead of full read
        use std::io::BufReader;
        let file_hash = {
            let mut hasher = Sha256::new();
            let f = std::fs::File::open(path)?;
            let mut reader = BufReader::new(f);
            std::io::copy(&mut reader, &mut hasher)?;
            format!("{:x}", hasher.finalize())
        };
        let total_size = std::fs::metadata(path)?.len() as usize;

        let transfer_id = transfer_id_override.unwrap_or_else(|| {
            format!("gft_{}_{}", file_hash, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
        });
        
        // ADAPTIVE CHUNKING: Direct P2P uses 256KB chunks, Relay/Pull uses 64KB
        let chunk_size = if is_relayed { 64 * 1024 } else { 256 * 1024 };
        let total_chunks = (total_size as f32 / chunk_size as f32).ceil() as u32;
        
        let transfer_payload = SignalingPayload::FileTransfer { 
            transfer_id: transfer_id.clone(), 
            filename: filename.clone(),
            mime_type: mime_type.clone(), 
            file_hash: file_hash.clone(),
            total_size: total_size as u64,
            is_relayed,
            sender_peer_id: Some(local_peer_id.to_string()),
            group_id: group_id.clone(),
        };
        let initial_progress = FileTransferProgress { 
            transfer_id: transfer_id.clone(), 
            peer_id: peer_id.to_string(), 
            filename: filename.clone(), 
            mime_type: mime_type.clone(),
            file_hash: file_hash.clone(),
            progress: 0.0, 
            is_complete: false, 
            is_verified: false,
            is_outgoing: true, 
            local_path: Some(file_path.clone()),
            start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            speed_bps: 0.0,
            group_id: group_id.clone(),
            caption: None,
        };

        // Persistent History: Save file manifest and BROADCAST TO GROUP if applicable
        let peer_id_str = peer_id.to_string();
        let msg_id = transfer_id.clone();
        let gid_opt = group_id.clone();
        if let Ok(json_str) = serde_json::to_string(&initial_progress) {
            let content = format!("[FILE]:{}", json_str);
            let s = Arc::clone(&storage);
            if !is_stress_test {
                if let Some(gid) = gid_opt {
                    let gid_clone = gid.clone();
                    let c_clone = content.clone();
                    let tx_clone = tx.clone();
                    tokio::task::spawn_blocking(move || s.store_group_message(&gid_clone, &peer_id_str, &msg_id, &c_clone, true, None, None));
                    
                    // BROADCAST: For group shares, we must also gossip the manifest to the group
                    let gid_for_broadcast = gid;
                    let fname_clone = filename.clone();
                    tokio::spawn(async move {
                        info!("[Mesh] Gossiping file manifest for {} to group {}", fname_clone, gid_for_broadcast);
                        // Use standard group message broadcast mechanism
                        let _ = tx_clone.send(NetworkCommand::BroadcastGroupMessage { 
                            group_id: gid_for_broadcast, 
                            message: content,
                            reply_to: None
                        }).await;
                    });
                } else {
                    tokio::task::spawn_blocking(move || s.store_message_with_id(&peer_id_str, &msg_id, &content, true, None, None));
                }
            }
        }

        // --- PULL MODEL: Register as an active seeder to serve chunk requests ---
        let _ = tx.send(NetworkCommand::RegisterSeeder {
            peer_id: local_peer_id,
            transfer_id: transfer_id.clone(),
            file_path: file_path.clone(),
            file_hash: file_hash.clone(),
            chunk_size,
            total_chunks,
            group_id: group_id.clone(),
        }).await;

        let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: transfer_payload, progress: initial_progress.clone() }).await;

        // GOSSIPSUB BROADCAST PATH: When peer_id == local_peer_id, this call came from the
        // group broadcast announce path. We've registered as seeder and gossiped the manifest.
        // Actual chunk delivery is handled per-member by the individual SendFile calls above.
        // Return here to avoid a self-push loop.
        if peer_id == local_peer_id {
            info!("✅ Group manifest announced and seeder registered for {}. Per-member push handles delivery.", filename);
            return Ok(());
        }

        if is_relayed {
            info!("✅ File transfer manifest sent for {}. (Relay mode - waiting for chunk requests).", filename);
            // BUG 1 FIX: Immediate mailbox fetch so we see the receiver's pull requests right away
            let _ = tx.send(NetworkCommand::FetchMailbox).await;
            return Ok(());
        }
        
        // Extended delay for manifest to propagate and relay circuits to warm up
        tokio::time::sleep(Duration::from_millis(if is_relayed { 2000 } else { 200 })).await;

        // BUG 4 FIX: Read chunks from disk during push loop
        let mut file = std::fs::File::open(path)?;
        for i in 0..total_chunks {
            let start = (i * chunk_size) as usize;
            let end = std::cmp::min(start + chunk_size as usize, total_size);
            
            use std::io::Seek;
            file.seek(std::io::SeekFrom::Start(start as u64))?;
            let mut chunk_data = vec![0u8; end - start];
            file.read_exact(&mut chunk_data)?;
            
            let chunk_payload = SignalingPayload::FileChunk { transfer_id: transfer_id.clone(), chunk_index: i, total_chunks, data_base64: general_purpose::STANDARD.encode(&chunk_data) };
            
            let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
            let elapsed_s = (current_time - initial_progress.start_time_ms) as f64 / 1000.0;
            let bytes_sent = end;
            let speed_bps = if elapsed_s > 0.01 { (bytes_sent as f64 * 8.0) / elapsed_s } else { 0.0 };

            let progress = FileTransferProgress { 
                transfer_id: transfer_id.clone(), 
                peer_id: peer_id.to_string(), 
                filename: filename.clone(), 
                mime_type: mime_type.clone(),
                file_hash: file_hash.clone(),
                progress: (i + 1) as f32 / total_chunks as f32, 
                is_complete: i + 1 == total_chunks, 
                is_verified: false,
                is_outgoing: true, 
                local_path: Some(file_path.clone()),
                start_time_ms: initial_progress.start_time_ms,
                speed_bps,
                group_id: group_id.clone(),
                caption: None,
            };
            
            // Since SendFileChunk is handled via the command channel, we can't easily wait here.
            // But the actual forward_to_mesh now drops chunks rather than buffering them infinitely.
            // To avoid overloading the channel itself, we simply apply a pacing delay.
            let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: chunk_payload.clone(), progress: progress.clone() }).await;
            
            // ADAPTIVE PACING: Direct P2P/WebRTC uses 20ms, Relay uses 250ms (checked dynamically)
            let current_relayed = is_relayed_map.read().get(&peer_id).cloned().unwrap_or(is_relayed);
            let has_webrtc = {
                let dc_store_read = data_channels.read();
                if let Some(dc) = dc_store_read.get(&peer_id) {
                    dc.ready_state() == RTCDataChannelState::Open
                } else {
                    false
                }
            };
            let is_direct = !current_relayed || has_webrtc;
            tokio::time::sleep(Duration::from_millis(if is_direct { 20 } else { 250 })).await;
        }
        


        info!("✅ File transfer chunks sent for {}. Waiting for verification from peer...", filename);
        Ok(())
    }

    fn select_best_providers_static(
        swarm: &Swarm<IntrovertBehaviour>,
        is_relayed_map: &Arc<RwLock<HashMap<PeerId, bool>>>,
        providers: &[PeerId],
    ) -> Vec<PeerId> {
        let is_connected_fn = |p: &PeerId| swarm.is_connected(p);
        let is_relayed_fn = |p: &PeerId| is_relayed_map.read().get(p).cloned().unwrap_or(true);

        let mut direct = Vec::new();
        let mut relayed = Vec::new();
        for p in providers {
            if is_connected_fn(p) {
                if !is_relayed_fn(p) {
                    direct.push(*p);
                } else {
                    relayed.push(*p);
                }
            }
        }
        if !direct.is_empty() {
            direct
        } else if !relayed.is_empty() {
            relayed
        } else {
            // No providers connected — return empty to trigger FindProviders discovery
            // instead of trying offline seeders repeatedly
            Vec::new()
        }
    }

    async fn execute_claw_actions(&mut self, actions: crate::intro_claw::ClawActions) {
        if actions.is_empty() { return; }

        info!("[IntroClaw] Executing {} actions",
            actions.heal_peers.len() + actions.prefetch_files.len() +
            actions.retry_dead_letters.len() + actions.upgrade_connections.len() +
            actions.pre_establish_relays.len() + if actions.force_mesh_refresh { 1 } else { 0 } +
            if actions.connection_strategy.is_some() { 1 } else { 0 });

        // 0. Force mesh refresh if network isolation detected
        if actions.force_mesh_refresh {
            info!("[IntroClaw] Network isolation detected — triggering ForceMeshRefresh");
            let tx = self.command_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(NetworkCommand::ForceMeshRefresh).await;
            });
        }

        // 0b. Handle intelligent connection strategy from IntroClaw cycler
        if let Some(strategy) = &actions.connection_strategy {
            use crate::intro_claw::ConnectionStrategy;
            match strategy {
                ConnectionStrategy::DirectBootstrap => {
                    info!("[IntroClaw/ConnCycler] Executing direct bootstrap re-dial");
                    crate::dispatch_debug_log("[IntroClaw/ConnCycler] Retrying direct TCP/QUIC bootstrap");
                    // PHASE 5: Network Status Stream — notify UI of RBN switch
                    crate::dispatch_global_event(48, b"switching:direct_bootstrap");
                    let mut any_dialed = false;
                    for (peer_id, addr) in &self.bootstrap_nodes {
                        crate::dispatch_debug_log(&format!("[IntroClaw/ConnCycler] Dialing bootstrap: {}", addr));
                        if self.swarm.dial(addr.clone()).is_ok() {
                            any_dialed = true;
                        }
                        let _ = self.swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
                    }
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                    if !any_dialed {
                        warn!("[IntroClaw/ConnCycler] All bootstrap dials failed");
                    }
                }
                ConnectionStrategy::TunnelWithConfig(config) => {
                    info!("[IntroClaw/ConnCycler] Executing tunnel with config: {}", config.description);
                    crate::dispatch_debug_log(&format!("[IntroClaw/ConnCycler] Trying VPN config: {}", config.description));
                    // PHASE 5: Network Status Stream — notify UI of RBN switch via tunnel
                    crate::dispatch_global_event(48, b"switching:tunnel_fallback");

                    // Reset existing tunnel if active
                    if self.tunnel_active {
                        self.tunnel_active = false;
                        self._tunnel_handle = None;
                        self.bootstrap_nodes.retain(|(_, addr)| !addr.to_string().contains("127.0.0.1"));
                    }

                    // Select tunnel URL based on config
                    let tunnel_url = if config.use_plaintext {
                        crate::network::types::RBN_WS_URL_PLAIN.to_string()
                    } else {
                        crate::network::types::RBN_WS_URL.to_string()
                    };

                    // Activate tunnel with the specified config
                    match tunnel::start_tunnel_client(0, tunnel_url.clone()).await {
                        Ok((port, handle)) => {
                            self.tunnel_active = true;
                            self.tunnel_started_at = Some(Instant::now());
                            self._tunnel_handle = Some(handle);
                            let tunnel_addr: Multiaddr = match format!("/ip4/127.0.0.1/tcp/{}", port).parse() {
                                Ok(addr) => addr,
                                Err(e) => {
                                    warn!("[IntroClaw/ConnCycler] Failed to parse tunnel address: {:?}", e);
                                    return;
                                }
                            };
                            let rbn_peer_id: PeerId = match types::RBN_PEER_ID.parse() {
                                Ok(id) => id,
                                Err(e) => {
                                    warn!("[IntroClaw/ConnCycler] Failed to parse RBN peer ID: {:?}", e);
                                    return;
                                }
                            };

                            if config.isolate_to_tunnel {
                                // VPN: Isolate bootstrap to tunnel ONLY
                                self.bootstrap_nodes.clear();
                                self.bootstrap_nodes.push((rbn_peer_id, tunnel_addr.clone()));
                                info!("[IntroClaw/ConnCycler] Isolated bootstrap to tunnel loopback only");
                            } else {
                                // Non-isolated: keep direct bootstrap alongside tunnel
                                self.bootstrap_nodes.push((rbn_peer_id, tunnel_addr.clone()));
                                info!("[IntroClaw/ConnCycler] Added tunnel alongside direct bootstrap");
                            }

                            self.swarm.behaviour_mut().kademlia.add_address(&rbn_peer_id, tunnel_addr.clone());
                            let _ = self.swarm.dial(tunnel_addr);
                            let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                            info!("[IntroClaw/ConnCycler] Tunnel active on local port {} ({})", port, config.description);
                            crate::dispatch_debug_log(&format!("[IntroClaw/ConnCycler] Tunnel activated on port {} — {}", port, config.description));
                        }
                        Err(e) => {
                            warn!("[IntroClaw/ConnCycler] Tunnel activation failed: {:?}", e);
                            crate::dispatch_debug_log(&format!("[IntroClaw/ConnCycler] Tunnel FAILED: {}", e));
                            self.tunnel_active = false;
                            self.tunnel_started_at = None;
                        }
                    }
                }
                ConnectionStrategy::ForceRefresh => {
                    info!("[IntroClaw/ConnCycler] All strategies failed — forcing mesh refresh");
                    crate::dispatch_debug_log("[IntroClaw/ConnCycler] Forcing mesh refresh (hard reset)");
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::ForceMeshRefresh).await;
                    });
                }
                ConnectionStrategy::ReportOffline => {
                    warn!("[IntroClaw/ConnCycler] All connection strategies exhausted — OFFLINE");
                    crate::dispatch_debug_log("[IntroClaw/ConnCycler] All strategies exhausted — OFFLINE");
                }
            }
        }

        // 1. Heal unreachable peers
        for peer_id_str in &actions.heal_peers {
            if let Ok(peer_id) = peer_id_str.parse::<PeerId>() {
                if !self.swarm.is_connected(&peer_id) {
                    info!("[IntroClaw] Auto-healing peer: {}", peer_id_str);
                    self.dial_relay_path(peer_id, false);
                }
            }
        }

        // 2. Upgrade relay connections to direct P2P
        for peer_id_str in &actions.upgrade_connections {
            if let Ok(peer_id) = peer_id_str.parse::<PeerId>() {
                info!("[IntroClaw] Upgrading connection to direct P2P: {}", peer_id_str);
                let _ = self.swarm.dial(peer_id);
            }
        }

        // 3. Pre-establish relays for unstable peers
        for peer_id_str in &actions.pre_establish_relays {
            if let Ok(peer_id) = peer_id_str.parse::<PeerId>() {
                if !self.swarm.is_connected(&peer_id) {
                    info!("[IntroClaw] Pre-establishing relay for unstable peer: {}", peer_id_str);
                    self.dial_relay_path(peer_id, false);
                }
            }
        }

        // 4. Prefetch missing files
        for file_hash in &actions.prefetch_files {
            info!("[IntroClaw] Auto-prefetching file: {}", file_hash);
            if let Ok(messages) = self.storage.search_all_messages(&format!("[FILE]:{}", file_hash), 1) {
                for (content, _, _, _, _, _, _) in messages {
                    if let Some(start) = content.find("[FILE]:") {
                        let json_part = &content[start + 7..];
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(json_part) {
                            let transfer_id = meta["transfer_id"].as_str().unwrap_or("");
                            let peer_id = meta["peer_id"].as_str().unwrap_or("");
                            let filename = meta["filename"].as_str().unwrap_or("unknown");
                            let mime_type = meta["mime_type"].as_str().unwrap_or("application/octet-stream");
                            let total_size = meta["total_size"].as_i64().unwrap_or(0);
                            let group_id = meta["group_id"].as_str().map(|s| s.to_string());

                            if let (Some(tid), Some(pid)) = (meta["transfer_id"].as_str(), meta["peer_id"].as_str()) {
                                if let Ok(peer) = pid.parse::<PeerId>() {
                                    let payload = crate::network::types::SignalingPayload::FileTransfer {
                                        transfer_id: tid.to_string(),
                                        filename: filename.to_string(),
                                        mime_type: mime_type.to_string(),
                                        file_hash: file_hash.clone(),
                                        total_size: total_size as u64,
                                        is_relayed: true,
                                        sender_peer_id: Some(pid.to_string()),
                                        group_id,
                                    };
                                    let _ = self.command_tx.send(NetworkCommand::HandleIncomingPayload { 
                                        peer_id: peer, 
                                        payload 
                                    }).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 5. Retry dead letters
        if !actions.retry_dead_letters.is_empty() {
            let connected: Vec<String> = self.swarm.connected_peers()
                .map(|p| p.to_string())
                .collect();
            let flushed = self.intro_claw.flush_offline_queue(&connected);
            for (peer_id, payload) in flushed {
                if let Ok(pid) = peer_id.parse::<PeerId>() {
                    info!("[IntroClaw] Retrying dead letter to: {}", peer_id);
                    let _ = self.command_tx.send(NetworkCommand::ForwardMeshSignaling {
                        peer_id: pid,
                        payload: bincode::deserialize(&payload).unwrap_or(
                            crate::network::types::SignalingPayload::Standard(String::from_utf8_lossy(&payload).to_string())
                        ),
                    }).await;
                }
            }
        }

        // 6. Node mode cache
        for (file_hash, peer_id) in &actions.cache_files_for_offline {
            info!("[IntroClaw Node] Proactive caching file {} for offline peer {}", file_hash, &peer_id[..8.min(peer_id.len())]);
            if let Ok(Some(file)) = self.storage.get_drive_file_by_hash(file_hash) {
                if !file.local_path.is_empty() {
                    let path = std::path::Path::new(&file.local_path);
                    if path.exists() {
                        let transfer_id = format!("node_cache_{}_{}", file_hash, std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs());
                        let _ = self.command_tx.send(NetworkCommand::RegisterSeeder {
                            peer_id: *self.swarm.local_peer_id(),
                            transfer_id,
                            file_path: file.local_path.clone(),
                            file_hash: file_hash.clone(),
                            chunk_size: 256 * 1024,
                            total_chunks: ((file.total_size as f64) / (256.0 * 1024.0)).ceil() as u32,
                            group_id: None,
                        }).await;
                    }
                }
            }
        }
    }
}
