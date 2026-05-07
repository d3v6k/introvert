use libp2p::{
    kad::{self, Record, RecordKey, QueryId},
    request_response,
    swarm::SwarmEvent,
    identity::Keypair,
    PeerId, Swarm, Multiaddr,
    futures::StreamExt,
};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use libp2p::{autonat, identify};

pub mod noise_session;
pub mod wormhole;
pub mod behaviour;
pub mod config;

use crate::media::{MediaManager, WebRtcSignal};
use noise_session::NoiseSession;
pub use behaviour::{IntrovertBehaviour, IntrovertBehaviourEvent};
use x25519_dalek::{StaticSecret, PublicKey};
use config::get_bootstrap_nodes;

// --- Signaling Types ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecureMessage {
    Handshake(Vec<u8>),
    Transport(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub sender_id: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalingPayload {
    Standard(String),
    WebRtc(WebRtcSignal),
    Secure(SecureMessage),
    MailboxStore { recipient_id: String, payload: Vec<u8> },
    MailboxDrain,
    MailboxDrained(Vec<MailboxMessage>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingRequest(pub SignalingPayload);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingResponse(pub String);

// --- Network Commands ---
pub enum NetworkCommand {
    Dial { peer_id: PeerId, address: Option<Multiaddr> },
    ListenOn { address: Multiaddr },
    SendSignaling { peer_id: PeerId, message: String },
    InitiateWebRtc { peer_id: PeerId },
    StartMediaStream { peer_id: PeerId, media_type: u8 },
    CloseWebRtc { peer_id: PeerId },
    RenegotiateWebRtc { peer_id: PeerId },
    AddAddress { peer_id: PeerId, address: Multiaddr },
    EstablishSecureSession { peer_id: PeerId },
    FetchMailbox,
}

// --- FFI Network Callback ---
pub type FfiNetworkCallback = extern "C" fn(event_type: i32, data_ptr: *const u8, data_len: usize);

pub struct NetworkService {
    swarm: Swarm<IntrovertBehaviour>,
    command_rx: mpsc::Receiver<NetworkCommand>,
    storage: Arc<crate::storage::StorageService>,
    peer_connections: Arc<RwLock<HashMap<PeerId, Arc<RTCPeerConnection>>>>,
    reward_tracker: Arc<crate::economy::RewardTracker>,
    local_static_secret: StaticSecret,
    local_static_public: PublicKey,
    session_encryption_key: [u8; 32],
    noise_sessions: HashMap<PeerId, NoiseSession>,
    pending_handshakes: HashMap<QueryId, PeerId>,
    pending_messages: HashMap<PeerId, Vec<String>>,
}

impl NetworkService {
    pub async fn new(
        keypair: Keypair, 
        _callback: FfiNetworkCallback,
        command_rx: mpsc::Receiver<NetworkCommand>,
        storage: Arc<crate::storage::StorageService>,
        reward_tracker: Arc<crate::economy::RewardTracker>,
        local_static_secret: StaticSecret,
        session_encryption_key: [u8; 32],
        enable_mdns: bool,
        enable_listeners: bool,
        tcp_port: u16,
        enable_relay_server: bool,
    ) -> anyhow::Result<Self> {
        let local_static_public = PublicKey::from(&local_static_secret);
        let local_peer_id = PeerId::from(keypair.public());

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )?
            .with_quic()
            .with_dns()?
            .with_relay_client(libp2p::noise::Config::new, libp2p::yamux::Config::default)?
            .with_behaviour(|keypair: &libp2p::identity::Keypair, relay_client| {
                IntrovertBehaviour::new(local_peer_id, keypair.public(), relay_client, enable_mdns, enable_relay_server)
            })?
            .with_swarm_config(|c: libp2p::swarm::Config| {
                c.with_idle_connection_timeout(Duration::from_secs(60))
            })
            .build();

        if enable_listeners {
            swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", tcp_port).parse()?)?;
            swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
            
            // Event 10: Local Node Status (1 = Online/Listening)
            let mut data = vec![1];
            data.shrink_to_fit();
            let ptr = data.as_ptr();
            let len = data.len();
            std::mem::forget(data);
            crate::dispatch_global_event(10, ptr, len);
        }
        
        Ok(Self { 
            swarm, 
            command_rx,
            storage,
            peer_connections: Arc::new(RwLock::new(HashMap::new())),
            reward_tracker,
            local_static_secret,
            local_static_public,
            session_encryption_key,
            noise_sessions: HashMap::new(),
            pending_handshakes: HashMap::new(),
            pending_messages: HashMap::new(),
        })
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

        let local_peer_id = *self.swarm.local_peer_id();
        let pubkey_record = Record {
            key: RecordKey::new(&local_peer_id.to_bytes()),
            value: self.local_static_public.to_bytes().to_vec(),
            publisher: Some(local_peer_id),
            expires: None,
        };
        let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

        for (peer_id, addr) in get_bootstrap_nodes() {
            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
        }
        
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
        let _ = self.handle_command(NetworkCommand::FetchMailbox).await;

        let mut republication_interval = tokio::time::interval(Duration::from_secs(15 * 60));
        let mut liveness_interval = tokio::time::interval(Duration::from_secs(5 * 60));
        let mut contact_refresh_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = republication_interval.tick() => {
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
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
                        eprintln!("Error handling swarm event: {:?}", e);
                    }
                }
                command = self.command_rx.recv() => {
                    if let Some(cmd) = command {
                        if let Err(e) = self.handle_command(cmd).await {
                            eprintln!("Command error: {:?}", e);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<IntrovertBehaviourEvent>) -> anyhow::Result<()> {
        match event {
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(list))) => {
                for (peer_id, addr) in list {
                    println!("mDNS discovered peer: {} at address: {}", peer_id, addr);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    let peer_id_str = peer_id.to_string();
                    match self.storage.get_contact(&peer_id_str) {
                        Ok(Some(_)) => {
                            println!("Dialing verified peer discovered via mDNS: {}", peer_id);
                            let _ = self.swarm.dial(peer_id);
                        },
                        _ => {}
                    }
                }
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Autonat(autonat::Event::StatusChanged { old: _, new })) => {
                println!("Reachability status changed: {:?}", new);
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Identify(identify::Event::Received { peer_id, info })) => {
                for addr in info.listen_addrs.clone() {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
                // Auto-Reserve on Bootstrap Nodes that support Relay V2 (Section 4: Relay Fallback)
                if info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop")) {
                    println!("Bootstrap node {} supports Relay HOP. Requesting reservation...", peer_id);
                    let relay_addr = libp2p::multiaddr::Multiaddr::empty()
                        .with(libp2p::multiaddr::Protocol::P2p(peer_id))
                        .with(libp2p::multiaddr::Protocol::P2pCircuit);
                    let _ = self.swarm.listen_on(relay_addr);
                }
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RelayClient(event)) => {
                match event {
                    libp2p::relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, .. } => {
                        println!("Relay reservation ACCEPTED by {}. Renewal: {}", relay_peer_id, renewal);
                        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

                        let mut data = vec![1]; // 1 = Connected (Relayed)
                        data.shrink_to_fit();
                        let ptr = data.as_ptr();
                        let len = data.len();
                        std::mem::forget(data);
                        crate::dispatch_global_event(8, ptr, len);

                        // Event 10: Local Node Status (2 = Relay Connected)
                        let mut data10 = vec![2];
                        data10.shrink_to_fit();
                        let ptr10 = data10.as_ptr();
                        let len10 = data10.len();
                        std::mem::forget(data10);
                        crate::dispatch_global_event(10, ptr10, len10);
                    }
                    _ => {
                        println!("RelayClient event: {:?}", event);
                    }
                }
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. })) => {
                let mut data = peer.to_bytes();
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                crate::dispatch_global_event(1, ptr, len); 
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetClosestPeers(Ok(kad::GetClosestPeersOk { key, peers })), .. })) => {
                if let Ok(target_peer) = PeerId::from_bytes(&key) {
                    if peers.contains(&target_peer) {
                         println!("DHT lookup found target peer {}. Dialing...", target_peer);
                         let _ = self.swarm.dial(target_peer);
                    }
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                let status: u8 = if endpoint.is_relayed() { 1 } else { 0 };
                let mut data = vec![status];
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                crate::dispatch_global_event(8, ptr, len);

                if endpoint.is_relayed() {
                    self.reward_tracker.record_relay(&peer_id.to_string(), 1024);
                }

                if let Some(messages) = self.pending_messages.remove(&peer_id) {
                    for msg in messages {
                        let _ = self.handle_command(NetworkCommand::SendSignaling { peer_id, message: msg }).await;
                    }
                }

                self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));

                let mut data = peer_id.to_bytes();
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                crate::dispatch_global_event(1, ptr, len); 
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { id, result: kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))), .. })) => {
                if let Some(peer_id) = self.pending_handshakes.remove(&id) {
                    let remote_static_pub: [u8; 32] = record.record.value.as_slice().try_into()?;
                    let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &remote_static_pub)?;
                    let handshake_msg = session.send_message(&[])?;
                    
                    let storage = Arc::clone(&self.storage);
                    let key = self.session_encryption_key;
                    let session_state = session.get_state();
                    tokio::spawn(async move {
                        let _ = NetworkService::persist_session_state(storage, key, peer_id, session_state).await;
                    });

                    self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                    self.noise_sessions.insert(peer_id, session);
                }
            }
            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Request { request, channel, .. }, .. })) => {
                // Send an ACK response immediately to satisfy the protocol requirement
                let _ = self.swarm.behaviour_mut().request_response.send_response(channel, SignalingResponse("ACK".to_string()));
                
                match request.0 {
                    SignalingPayload::WebRtc(signal) => {
                        match signal.signal_type.as_str() {
                            "offer" => {
                                let pc = MediaManager::create_peer_connection(false, Arc::clone(&self.reward_tracker), peer).await?;
                                let answer_sdp = MediaManager::handle_offer(signal.sdp, Arc::clone(&pc)).await?;
                                self.peer_connections.write().insert(peer, pc);
                                let response = WebRtcSignal { signal_type: "answer".to_owned(), sdp: answer_sdp };
                                self.swarm.behaviour_mut().request_response.send_request(&peer, SignalingRequest(SignalingPayload::WebRtc(response)));
                            }
                            "answer" => {
                                let pc_opt = self.peer_connections.read().get(&peer).cloned();
                                if let Some(pc) = pc_opt {
                                    MediaManager::handle_answer(signal.sdp, pc).await?;
                                }
                            }
                            _ => {}
                        }
                    }
                    SignalingPayload::Secure(secure_msg) => {
                        match secure_msg {
                            SecureMessage::Handshake(payload) => {
                                if let Some(session) = self.noise_sessions.get_mut(&peer) {
                                    session.recv_message(&payload)?;
                                    println!("E2EE Handshake COMPLETED with peer: {}", peer);
                                } else {
                                    let mut session = NoiseSession::responder(self.local_static_secret.to_bytes().as_slice())?;
                                    let _response = session.send_message(&[])?; // Just an empty handshake response if Noise IK
                                    // Actually Noise IK initiator sends first message, responder reads and then sends response
                                    // Let's refine based on noise_session.rs which handles the state internally
                                    let _ = session.recv_message(&payload)?;
                                    let response = session.send_message(&[])?;
                                    
                                    self.noise_sessions.insert(peer, session);
                                    self.swarm.behaviour_mut().request_response.send_request(&peer, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(response))));
                                }
                            }
                            SecureMessage::Transport(encrypted) => {
                                if let Some(session) = self.noise_sessions.get_mut(&peer) {
                                    match session.recv_message(&encrypted) {
                                        Ok(decrypted) => {
                                            if decrypted.starts_with(b"WEBRTC:") {
                                                if let Ok(signal) = serde_json::from_slice::<WebRtcSignal>(&decrypted[7..]) {
                                                    match signal.signal_type.as_str() {
                                                        "offer" => {
                                                            let pc = MediaManager::create_peer_connection(false, Arc::clone(&self.reward_tracker), peer).await?;
                                                            let answer_sdp = MediaManager::handle_offer(signal.sdp, Arc::clone(&pc)).await?;
                                                            self.peer_connections.write().insert(peer, pc);
                                                            let response = WebRtcSignal { signal_type: "answer".to_owned(), sdp: answer_sdp };
                                                            let mut payload = b"WEBRTC:".to_vec();
                                                            payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                                                            let encrypted_resp = session.send_message(&payload)?;
                                                            self.swarm.behaviour_mut().request_response.send_request(&peer, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted_resp))));
                                                        }
                                                        "answer" => {
                                                            let pc_opt = self.peer_connections.read().get(&peer).cloned();
                                                            if let Some(pc) = pc_opt {
                                                                MediaManager::handle_answer(signal.sdp, pc).await?;
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            } else {
                                                let ptr = decrypted.as_ptr();
                                                let len = decrypted.len();
                                                std::mem::forget(decrypted);
                                                crate::dispatch_global_event(2, ptr, len); 
                                            }
                                        }
                                        Err(_) => {
                                            self.noise_sessions.remove(&peer);
                                            let _ = self.handle_command(NetworkCommand::EstablishSecureSession { peer_id: peer }).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    SignalingPayload::Standard(msg) => {
                        let data = msg.into_bytes();
                        let ptr = data.as_ptr();
                        let len = data.len();
                        std::mem::forget(data);
                        crate::dispatch_global_event(2, ptr, len); 
                    }
                    SignalingPayload::MailboxStore { recipient_id, payload } => {
                        if let Ok(recipient) = recipient_id.parse::<PeerId>() {
                            let _ = self.storage.store_mailbox_payload(&recipient, &peer, payload);
                        }
                    }
                    SignalingPayload::MailboxDrain => {
                        if let Ok(messages) = self.storage.drain_mailbox(&peer) {
                            self.swarm.behaviour_mut().request_response.send_request(&peer, SignalingRequest(SignalingPayload::MailboxDrained(messages)));
                        }
                    }
                    SignalingPayload::MailboxDrained(messages) => {
                        for msg in messages {
                            if let Ok(sender_peer) = msg.sender_id.parse::<PeerId>() {
                                if let Ok(signaling) = serde_json::from_slice::<SignalingPayload>(&msg.payload) {
                                    match signaling {
                                        SignalingPayload::Secure(secure) => {
                                            if let (SecureMessage::Transport(encrypted), Some(session)) = (secure, self.noise_sessions.get_mut(&sender_peer)) {
                                                if let Ok(decrypted) = session.recv_message(&encrypted) {
                                                    let ptr = decrypted.as_ptr();
                                                    let len = decrypted.len();
                                                    std::mem::forget(decrypted);
                                                    crate::dispatch_global_event(4, ptr, len); 
                                                }
                                            }
                                        }
                                        SignalingPayload::Standard(text) => {
                                            let data = text.into_bytes();
                                            let ptr = data.as_ptr();
                                            let len = data.len();
                                            std::mem::forget(data);
                                            crate::dispatch_global_event(4, ptr, len);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                let mut data = vec![2]; // 2 = Offline
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                crate::dispatch_global_event(8, ptr, len);

                if let Some(pid) = peer_id {
                    println!("Outgoing connection failed for peer: {} with error: {:?}", pid, error);
                } else {
                    println!("Outgoing connection failed with error: {:?}", error);
                }
            }

            _ => {}
        }
        Ok(())
    }

    async fn handle_command(&mut self, command: NetworkCommand) -> anyhow::Result<()> {
        match command {
            NetworkCommand::Dial { peer_id, address } => {
                if let Some(addr) = address {
                    let final_addr = if addr.iter().any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_))) { addr } else { addr.with(libp2p::multiaddr::Protocol::P2p(peer_id)) };
                    self.swarm.dial(final_addr)?;
                } else {
                    if let Err(e) = self.swarm.dial(peer_id) {
                        println!("Direct dial failed for {}: {:?}. Triggering DHT lookup...", peer_id, e);
                        let _ = self.swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
                    }
                }
            }
            NetworkCommand::ListenOn { address } => { self.swarm.listen_on(address)?; }
            NetworkCommand::SendSignaling { peer_id, message } => {
                let message_len = message.len() as u64;
                if self.swarm.is_connected(&peer_id) {
                    let mut sent_secure = false;
                    if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                        if session.is_finished() {
                            self.reward_tracker.record_relay(&peer_id.to_string(), message_len);
                            let encrypted = session.send_message(message.as_bytes())?;
                            self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted))));
                            sent_secure = true;
                        }
                    }
                    if !sent_secure {
                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Standard(message)));
                    }
                } else {
                    self.pending_messages.entry(peer_id).or_default().push(message.clone());
                    let mut anchor_nodes = self.storage.fetch_all_anchor_nodes().unwrap_or_default();
                    anchor_nodes.retain(|n| if let Ok(pid) = n.peer_id.parse::<PeerId>() { self.swarm.is_connected(&pid) } else { false });
                    let target_anchor = if let Some(node) = anchor_nodes.first() { node.peer_id.parse::<PeerId>().ok() } else { self.swarm.connected_peers().next().cloned() };
                    
                    if let Some(anchor_id) = target_anchor {
                        let payload = if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                            if session.is_finished() {
                                let encrypted = session.send_message(message.as_bytes())?;
                                SignalingPayload::Secure(SecureMessage::Transport(encrypted))
                            } else { SignalingPayload::Standard(message) }
                        } else { SignalingPayload::Standard(message) };
                        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
                        self.swarm.behaviour_mut().request_response.send_request(&anchor_id, SignalingRequest(SignalingPayload::MailboxStore { recipient_id: peer_id.to_string(), payload: bytes }));
                    }

                    // Fallback to DHT lookup if direct dial fails
                    if let Err(e) = self.swarm.dial(peer_id) {
                        println!("SendSignaling: Dial failed for {}: {:?}. Triggering DHT lookup...", peer_id, e);
                        let _ = self.swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
                    }
                    
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                }
            }
            NetworkCommand::FetchMailbox => {
                let anchor_nodes = self.storage.fetch_all_anchor_nodes().unwrap_or_default();
                for node in anchor_nodes {
                    if let Ok(peer_id) = node.peer_id.parse::<PeerId>() {
                        if self.swarm.is_connected(&peer_id) {
                            self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));
                        } else { let _ = self.swarm.dial(peer_id); }
                    }
                }
            }
            NetworkCommand::InitiateWebRtc { peer_id } => {
                let pc = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id).await?;
                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        let encrypted = session.send_message(&payload)?;
                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted))));
                        is_secure = true;
                    }
                }
                if !is_secure { self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::WebRtc(signal))); }
            }
            NetworkCommand::StartMediaStream { peer_id, media_type } => {
                let pc_clone = {
                    let pcs = self.peer_connections.read();
                    pcs.get(&peer_id).cloned()
                };
                if let Some(pc) = pc_clone {
                    MediaManager::add_media_tracks(pc, media_type).await?;
                }
            }
            NetworkCommand::CloseWebRtc { peer_id } => { 
                let pc = {
                    let mut pcs = self.peer_connections.write();
                    pcs.remove(&peer_id)
                };
                if let Some(pc) = pc { 
                    let _ = pc.close().await; 
                } 
            }
            NetworkCommand::RenegotiateWebRtc { peer_id } => { 
                let pc = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id).await?;
                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        let encrypted = session.send_message(&payload)?;
                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted))));
                        is_secure = true;
                    }
                }
                if !is_secure { self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::WebRtc(signal))); }
            }
            NetworkCommand::AddAddress { peer_id, address } => { self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address); }
            NetworkCommand::EstablishSecureSession { peer_id } => {
                if self.noise_sessions.contains_key(&peer_id) { return Ok(()); }
                let peer_id_str = peer_id.to_string();
                let storage = Arc::clone(&self.storage);
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
                                if let Ok(session) = crate::network::noise_session::NoiseSession::from_state(state) {
                                    self.noise_sessions.insert(peer_id, session);
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                let storage_contact = storage.clone();
                let contact = tokio::task::spawn_blocking(move || storage_contact.get_contact(&peer_id_str)).await??;
                if let Some(identity) = contact {
                    let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &identity.static_key)?;
                    let handshake_msg = session.send_message(&[])?;
                    let storage_save = Arc::clone(&self.storage);
                    let enc_key = self.session_encryption_key;
                    let session_state = session.get_state();
                    tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage_save, enc_key, peer_id, session_state).await; });
                    self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                    self.noise_sessions.insert(peer_id, session);
                } else {
                    let key = RecordKey::new(&peer_id.to_bytes());
                    let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);
                    self.pending_handshakes.insert(query_id, peer_id);
                }
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
}
