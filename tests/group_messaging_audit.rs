use anyhow::Result;
use introvert::identity::{NodeIdentity, SovereignIdentity};
use introvert::network::{NetworkCommand, NetworkConfig, NetworkService, GroupMemberMetadata, GroupRole, GroupAction, SignalingPayload};
use introvert::storage::StorageService;
use introvert::economy::RewardTracker;
use libp2p::{PeerId, SwarmBuilder, futures::StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::sleep;
use parking_lot::Mutex;
use once_cell::sync::Lazy;

static RECEIVED_MESSAGES: Lazy<Mutex<Vec<(String, String)>>> = Lazy::new(|| Mutex::new(Vec::new()));

extern "C" fn audit_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 21 {
        // Event 21 = Group Message Received
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        // Format of event 21 data: [group_id_len][group_id][signer_len][signer][reply_to_len][reply_to][content]
        let mut offset = 0;
        if data_len > 0 {
            let group_id_len = data_slice[offset] as usize;
            offset += 1;
            if offset + group_id_len <= data_len {
                let group_id = String::from_utf8_lossy(&data_slice[offset..offset+group_id_len]).into_owned();
                offset += group_id_len;
                
                let signer_len = data_slice[offset] as usize;
                offset += 1;
                if offset + signer_len <= data_len {
                    let _signer = String::from_utf8_lossy(&data_slice[offset..offset+signer_len]).into_owned();
                    offset += signer_len;
                    
                    let reply_to_len = data_slice[offset] as usize;
                    offset += 1;
                    if offset + reply_to_len <= data_len {
                        let _reply_to = String::from_utf8_lossy(&data_slice[offset..offset+reply_to_len]).into_owned();
                        offset += reply_to_len;
                        
                        let content = String::from_utf8_lossy(&data_slice[offset..]).into_owned();
                        RECEIVED_MESSAGES.lock().push((group_id, content));
                    }
                }
            }
        }
    }
    introvert::introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_group_messaging_reliability() -> Result<()> {
    std::env::set_var("INTROVERT_SKIP_BOOTSTRAP", "1");
    println!("📡 Starting Group Messaging Audit...");
    *introvert::TEST_CALLBACK.write() = Some(audit_callback);

    // 1. Setup Relay Node (Pure libp2p for simplicity)
    let relay_keypair = libp2p::identity::Keypair::generate_ed25519();
    let relay_peer_id = PeerId::from(relay_keypair.public());
    
    #[derive(libp2p::swarm::NetworkBehaviour)]
    struct RelayBehaviour {
        relay: libp2p::relay::Behaviour,
        identify: libp2p::identify::Behaviour,
    }

    let mut relay_swarm = SwarmBuilder::with_existing_identity(relay_keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(|keypair| RelayBehaviour {
            relay: libp2p::relay::Behaviour::new(
                relay_peer_id,
                libp2p::relay::Config {
                    max_circuit_bytes: 1024 * 1024 * 1024,
                    max_circuit_duration: std::time::Duration::from_secs(60 * 60),
                    max_reservations: 256,
                    max_circuits: 128,
                    ..Default::default()
                },
            ),
            identify: libp2p::identify::Behaviour::new(libp2p::identify::Config::new(
                "/introvert/relay/1.0.0".to_string(),
                keypair.public(),
            )),
        })?
        .build();

    relay_swarm.listen_on("/ip4/127.0.0.1/tcp/0".parse()?)?;
    
    let mut relay_addr = loop {
        if let libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } = relay_swarm.select_next_some().await {
            break address;
        }
    };
    relay_swarm.add_external_address(relay_addr.clone());
    relay_addr = relay_addr.with(libp2p::multiaddr::Protocol::P2p(relay_peer_id));
    println!("Relay Node online at: {}", relay_addr);

    tokio::spawn(async move {
        loop {
            relay_swarm.select_next_some().await;
        }
    });

    // 2. Setup Node A (Sender)
    let temp_dir_a = TempDir::new()?;
    let mut seed_a = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut seed_a);
    let id_a = NodeIdentity::from_seed(seed_a)?;
    let storage_a = Arc::new(StorageService::new(temp_dir_a.path().join("a.db"), &NodeIdentity::derive_storage_key(seed_a)?)?);
    let tracker_a = Arc::new(RewardTracker::new(Some(storage_a.clone())));
    let (cmd_tx_a, cmd_rx_a) = mpsc::channel(100);
    let service_a = NetworkService::new(NetworkConfig {
        keypair: id_a.keypair.clone(),
        command_rx: cmd_rx_a,
        command_tx: cmd_tx_a.clone(),
        storage: storage_a.clone(),
        reward_tracker: tracker_a.clone(),
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new("https://api.devnet.solana.com", "11111111111111111111111111111111", "dummy").unwrap()),
        local_static_secret: NodeIdentity::derive_e2ee_key(seed_a)?,
        session_encryption_key: NodeIdentity::derive_session_encryption_key(seed_a)?,
        enable_mdns: false,
        enable_listeners: false,
        tcp_port: 0,
        enable_relay_server: false,
        max_connections: 100_000,
        liveness_interval_secs: 600,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: false,
    }).await?;
    let peer_id_a = id_a.peer_id;
    tokio::spawn(service_a.run());

    // 3. Setup Node B (Receiver)
    let temp_dir_b = TempDir::new()?;
    let mut seed_b = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut seed_b);
    let id_b = NodeIdentity::from_seed(seed_b)?;
    let storage_b = Arc::new(StorageService::new(temp_dir_b.path().join("b.db"), &NodeIdentity::derive_storage_key(seed_b)?)?);
    let tracker_b = Arc::new(RewardTracker::new(Some(storage_b.clone())));
    let (cmd_tx_b, cmd_rx_b) = mpsc::channel(100);
    let service_b = NetworkService::new(NetworkConfig {
        keypair: id_b.keypair.clone(),
        command_rx: cmd_rx_b,
        command_tx: cmd_tx_b.clone(),
        storage: storage_b.clone(),
        reward_tracker: tracker_b.clone(),
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new("https://api.devnet.solana.com", "11111111111111111111111111111111", "dummy").unwrap()),
        local_static_secret: NodeIdentity::derive_e2ee_key(seed_b)?,
        session_encryption_key: NodeIdentity::derive_session_encryption_key(seed_b)?,
        enable_mdns: false,
        enable_listeners: false,
        tcp_port: 0,
        enable_relay_server: false,
        max_connections: 100_000,
        liveness_interval_secs: 600,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: false,
    }).await?;
    let peer_id_b = id_b.peer_id;
    tokio::spawn(service_b.run());

    // 4. Connect both to Relay
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: relay_peer_id, address: None }).await?;
    cmd_tx_b.send(NetworkCommand::Dial { peer_id: relay_peer_id, address: None }).await?;
    sleep(Duration::from_secs(3)).await;

    // Also make each node aware of the other's relay address
    let relay_a_addr = relay_addr.clone()
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(peer_id_a));
    cmd_tx_b.send(NetworkCommand::AddAddress { peer_id: peer_id_a, address: relay_a_addr }).await?;

    // Node B listens on Relay
    let relay_circuit_b = relay_addr.clone()
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(peer_id_b));
    cmd_tx_b.send(NetworkCommand::ListenOn { address: relay_circuit_b }).await?;
    sleep(Duration::from_secs(2)).await;

    // Node A connects to Node B via Relay
    let relay_b_addr = relay_addr.clone()
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(peer_id_b));
    
    println!("Node A establishing relayed connection to Node B: {}", relay_b_addr);
    cmd_tx_a.send(NetworkCommand::AddAddress { peer_id: peer_id_b, address: relay_b_addr.clone() }).await?;
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: peer_id_b, address: Some(relay_b_addr) }).await?;
    sleep(Duration::from_secs(5)).await;

    // 5. Populate Group and Contacts in database
    let group_id = "test_group_123".to_string();
    let name = "Test Group".to_string();
    let description = "Group Description".to_string();
    let group_secret = [3u8; 32];
    
    // Creator member metadata (Node A)
    let creator_member = GroupMemberMetadata {
        peer_id: peer_id_a.to_string(),
        pubkey: id_a.keypair.public().encode_protobuf(),
        role: GroupRole::Creator,
        alias: Some("Alice".to_string()),
        avatar_base64: None,
    };
    
    // Member metadata (Node B)
    let member_b = GroupMemberMetadata {
        peer_id: peer_id_b.to_string(),
        pubkey: id_b.keypair.public().encode_protobuf(),
        role: GroupRole::Member,
        alias: Some("Bob".to_string()),
        avatar_base64: None,
    };
    
    let members = vec![creator_member, member_b];
    let members_json = serde_json::to_string(&members)?;
    
    // Save group on both Node A and Node B
    storage_a.upsert_group(&group_id, &name, &description, &members_json)?;
    storage_a.save_group_secret(&group_id, &group_secret)?;
    
    storage_b.upsert_group(&group_id, &name, &description, &members_json)?;
    storage_b.save_group_secret(&group_id, &group_secret)?;

    let static_secret_a = NodeIdentity::derive_e2ee_key(seed_a)?;
    let static_public_a = x25519_dalek::PublicKey::from(&static_secret_a).to_bytes();
    
    let static_secret_b = NodeIdentity::derive_e2ee_key(seed_b)?;
    let static_public_b = x25519_dalek::PublicKey::from(&static_secret_b).to_bytes();

    // Construct SovereignIdentity
    let sov_a = SovereignIdentity {
        peer_id: peer_id_a.to_string(),
        p2p_pubkey: id_a.keypair.public().encode_protobuf(),
        static_key: static_public_a,
        solana_address: "11111111111111111111111111111111".to_string(),
        global_name: Some("Alice".to_string()),
        local_alias: Some("Alice".to_string()),
        avatar_base64: None,
        is_anchor_capable: false,
        retention_seconds: 86400,
        handle: Some("i@alice".to_string()),
        prestige_tier: Some(0),
    };
    
    let sov_b = SovereignIdentity {
        peer_id: peer_id_b.to_string(),
        p2p_pubkey: id_b.keypair.public().encode_protobuf(),
        static_key: static_public_b,
        solana_address: "11111111111111111111111111111112".to_string(),
        global_name: Some("Bob".to_string()),
        local_alias: Some("Bob".to_string()),
        avatar_base64: None,
        is_anchor_capable: false,
        retention_seconds: 86400,
        handle: Some("i@bob".to_string()),
        prestige_tier: Some(0),
    };

    // Upsert contacts
    storage_a.upsert_sovereign_contact(&sov_b, true, false)?;
    storage_b.upsert_sovereign_contact(&sov_a, true, false)?;

    // 6. Node A sends Group Message to Node B
    println!("Node A sending group message...");
    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
    use rand::RngCore;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_secret));
    
    let message_text = "Hello group members!";
    let encrypted = cipher.encrypt(Nonce::from_slice(&nonce_bytes), message_text.as_bytes()).unwrap();
    let mut content_encrypted = nonce_bytes.to_vec();
    content_encrypted.extend(encrypted);
    
    let action = GroupAction::Message {
        content_encrypted,
        msg_id: "gm_test_1".to_string(),
        reply_to: None,
    };
    
    let signed = introvert::network::group::GroupManager::sign_action(group_id.clone(), action, &id_a.keypair)?;
    let payload = SignalingPayload::GroupAction(signed);
    
    // Send message to B in real-time
    cmd_tx_a.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer_id_b, payload }).await?;

    // 7. Wait and verify B received the message via FFI event callback (event_type=21)
    let mut received = false;
    for _ in 0..10 {
        sleep(Duration::from_secs(1)).await;
        let msgs = RECEIVED_MESSAGES.lock();
        if msgs.iter().any(|(gid, content)| gid == &group_id && content == message_text) {
            received = true;
            break;
        }
    }

    assert!(received, "Receiver Node B did not receive the real-time group message");
    println!("✅ Group message delivered successfully in real-time!");

    Ok(())
}
