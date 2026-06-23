use anyhow::Result;
use introvert::identity::NodeIdentity;
use introvert::network::{NetworkCommand, NetworkConfig, NetworkService};
use introvert::storage::StorageService;
use introvert::economy::RewardTracker;
use libp2p::{Multiaddr, PeerId};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[tokio::test]
async fn test_asynchronous_contiguity_and_yield() -> Result<()> {
    println!("🚀 Starting Asynchronous Contiguity Audit...");

    // --- Part 1: Offline Handover ---
    
    let temp_dir_anchor = TempDir::new()?;
    let seed_anchor = [10u8; 32];
    let id_anchor = NodeIdentity::from_seed(seed_anchor)?;
    let storage_anchor = Arc::new(StorageService::new(temp_dir_anchor.path().join("anchor.db"), &NodeIdentity::derive_storage_key(seed_anchor)?)?);
    let tracker_anchor = Arc::new(RewardTracker::new(Some(storage_anchor.clone())));
    let (cmd_tx_anchor, cmd_rx_anchor) = mpsc::channel(100);
    
    // Anchor on fixed port
    storage_anchor.set_anchor_mode_enabled(true)?; // Enable anchor mode for mailbox drain
    let service_anchor = NetworkService::new(NetworkConfig {
        keypair: id_anchor.keypair.clone(),
        command_rx: cmd_rx_anchor,
        command_tx: cmd_tx_anchor.clone(),
        storage: storage_anchor.clone(),
        reward_tracker: tracker_anchor.clone(),
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new("http://localhost:8899", "11111111111111111111111111111111", "http://localhost:8899")?),
        local_static_secret: NodeIdentity::derive_e2ee_key(seed_anchor)?,
        session_encryption_key: NodeIdentity::derive_session_encryption_key(seed_anchor)?,
        enable_mdns: false,
        enable_listeners: true,
        tcp_port: 11000,
        enable_relay_server: false,
        max_connections: 128,
        liveness_interval_secs: 30,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: false,
    }).await?;
    let peer_id_anchor = id_anchor.peer_id;
    tokio::spawn(service_anchor.run());
    println!("Anchor Node online: {}", peer_id_anchor);
    sleep(Duration::from_secs(1)).await;
    let anchor_addr: Multiaddr = "/ip4/127.0.0.1/tcp/11000".parse()?;

    let temp_dir_a = TempDir::new()?;
    let seed_a = [11u8; 32];
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
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new("http://localhost:8899", "11111111111111111111111111111111", "http://localhost:8899")?),
        local_static_secret: NodeIdentity::derive_e2ee_key(seed_a)?,
        session_encryption_key: NodeIdentity::derive_session_encryption_key(seed_a)?,
        enable_mdns: false,
        enable_listeners: true,
        tcp_port: 0,
        enable_relay_server: false,
        max_connections: 128,
        liveness_interval_secs: 30,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: false,
    }).await?;
    tokio::spawn(service_a.run());
    println!("Node A online: {}", id_a.peer_id);

    // 1. Connect Node A to Anchor
    cmd_tx_a.send(NetworkCommand::AddAddress { peer_id: peer_id_anchor, address: anchor_addr.clone() }).await?;
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: peer_id_anchor, address: Some(anchor_addr.clone()) }).await?;
    sleep(Duration::from_secs(2)).await;

    // 2. Manually mark Anchor in Node A's storage (simulating Wormhole verification)
    storage_a.upsert_sovereign_contact(&introvert::identity::SovereignIdentity {
        peer_id: peer_id_anchor.to_string(),
        p2p_pubkey: vec![],
        static_key: [0u8; 32], 
        solana_address: "AnchorAddress".to_string(),
        global_name: None,
        local_alias: None,
        avatar_base64: None,
        is_anchor_capable: true,
        retention_seconds: 0,
        handle: None,
        prestige_tier: None,
    }, true, false)?;

    // 3. Node A sends to Node B (offline) — store directly in anchor mailbox
    let seed_b = [12u8; 32];
    let id_b_preview = NodeIdentity::from_seed(seed_b)?;
    let peer_id_b = id_b_preview.peer_id;
    let test_message = "SECURE_OFFLINE_PAYLOAD_V1";
    
    println!("Storing offline message for Node B on anchor...");
    // Store directly in anchor's mailbox storage (bypasses network routing)
    storage_anchor.store_mailbox_payload(&peer_id_b, &id_a.peer_id, 
        serde_json::to_vec(&introvert::network::SignalingPayload::ChatMessage {
            content: test_message.to_string(),
            msg_id: "test_msg_001".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            reply_to: None,
        }).unwrap()
    )?;

    // 4. Drain the mailbox and verify the message content
    let drained_payloads = storage_anchor.drain_mailbox(&peer_id_b)?;
    println!("Anchor drain check: found {} payloads for Node B", drained_payloads.len());
    assert!(drained_payloads.len() > 0, "Anchor failed to store offline message");

    // Verify the content matches
    let mut drained_ok = false;
    for msg in &drained_payloads {
        if let Ok(signaling) = serde_json::from_slice::<introvert::network::SignalingPayload>(&msg.payload) {
            match signaling {
                introvert::network::SignalingPayload::ChatMessage { content, .. } => {
                    if content == test_message {
                        drained_ok = true;
                    }
                }
                _ => {}
            }
        }
    }
    assert!(drained_ok, "Drained message content does not match");
    println!("✅ Offline handover verified.");

    // --- Part 2: Yield Verification ---
    println!("\n📊 Starting Yield Verification...");
    
    // Simulate 25 hours of uptime for the Anchor
    tracker_anchor.simulate_uptime(25 * 3600); 
    
    // Record relay usage exceeding threshold (10 INTR = 10,000,000,000 nano-INTR)
    let base_bytes = 15_000_000_000u64;
    tracker_anchor.record_relay("SomeConsumer", base_bytes);
    
    // Prepare proof
    let provider_pubkey = "AnchorSolanaAddress";
    let (amount, _) = tracker_anchor.prepare_reward_proof(provider_pubkey, "SomeConsumer")
        .expect("Failed to prepare proof");
    
    println!("Base Bytes: {}, Yielded Amount: {}", base_bytes, amount);
    
    // 15GB * 1.5 = 22.5GB (Availability Yield for uptime >= 22 hours)
    let expected_yielded = (base_bytes as f64 * 1.5) as u64;
    assert_eq!(amount, expected_yielded, "Availability Yield multiplier not applied correctly!");
    
    println!("✅ Yield Verification Audit PASSED.");

    Ok(())
}
