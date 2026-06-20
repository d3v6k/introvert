use anyhow::Result;
use introvert::identity::NodeIdentity;
use introvert::network::{NetworkCommand, NetworkConfig, NetworkService, SignalingPayload, SecureMessage};
use introvert::storage::StorageService;
use introvert::economy::RewardTracker;
use libp2p::{Multiaddr, PeerId};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::sleep;
use std::sync::atomic::{AtomicUsize, Ordering};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

static DRAINED_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

extern "C" fn audit_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 4 {
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        let msg = String::from_utf8_lossy(data_slice).to_string();
        DRAINED_MESSAGES.lock().push(msg);
    }
    introvert::introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[tokio::test]
async fn test_asynchronous_contiguity_and_yield() -> Result<()> {
    println!("🚀 Starting Asynchronous Contiguity Audit...");
    *introvert::TEST_CALLBACK.write() = Some(audit_callback);

    // --- Part 1: Offline Handover ---
    
    let temp_dir_anchor = TempDir::new()?;
    let seed_anchor = [10u8; 32];
    let id_anchor = NodeIdentity::from_seed(seed_anchor)?;
    let storage_anchor = Arc::new(StorageService::new(temp_dir_anchor.path().join("anchor.db"), &NodeIdentity::derive_storage_key(seed_anchor)?)?);
    let tracker_anchor = Arc::new(RewardTracker::new(Some(storage_anchor.clone())));
    let (cmd_tx_anchor, cmd_rx_anchor) = mpsc::channel(100);
    
    // Anchor on fixed port
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
        static_key: [0u8; 32], 
        solana_address: "AnchorAddress".to_string(),
        is_anchor_capable: true,
    })?;

    // 3. Node A sends to Node B (offline)
    let seed_b = [12u8; 32];
    let id_b_preview = NodeIdentity::from_seed(seed_b)?;
    let peer_id_b = id_b_preview.peer_id;
    let test_message = "SECURE_OFFLINE_PAYLOAD_V1";
    
    println!("Node A attempting to send to offline Node B: {}...", peer_id_b);
    cmd_tx_a.send(NetworkCommand::SendSignaling { 
        peer_id: peer_id_b, 
        message: test_message.to_string() 
    }).await?;

    // Wait for anchor to store it
    sleep(Duration::from_secs(3)).await;

    // 4. Verification: Check Anchor storage
    let payloads = storage_anchor.fetch_mailbox_payloads(&peer_id_b)?;
    println!("Anchor storage check: found {} payloads for Node B", payloads.len());
    assert!(payloads.len() > 0, "Anchor failed to store offline message");

    // Put it back for Node B to drain
    storage_anchor.store_mailbox_payload(&peer_id_b, &id_a.peer_id, payloads[0].1.clone())?;

    // 5. Start Node B
    let temp_dir_b = TempDir::new()?;
    let id_b = NodeIdentity::from_seed(seed_b)?;
    let storage_b = Arc::new(StorageService::new(temp_dir_b.path().join("b.db"), &NodeIdentity::derive_storage_key(seed_b)?)?);
    let tracker_b = Arc::new(RewardTracker::new(Some(storage_b.clone())));
    let (cmd_tx_b, cmd_rx_b) = mpsc::channel(100);
    
    // Manually add Anchor to Node B's storage
    storage_b.upsert_sovereign_contact(&introvert::identity::SovereignIdentity {
        peer_id: peer_id_anchor.to_string(),
        static_key: [0u8; 32], 
        solana_address: "AnchorAddress".to_string(),
        is_anchor_capable: true,
    })?;

    let service_b = NetworkService::new(NetworkConfig {
        keypair: id_b.keypair.clone(),
        command_rx: cmd_rx_b,
        command_tx: cmd_tx_b.clone(),
        storage: storage_b.clone(),
        reward_tracker: tracker_b.clone(),
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new("http://localhost:8899", "11111111111111111111111111111111", "http://localhost:8899")?),
        local_static_secret: NodeIdentity::derive_e2ee_key(seed_b)?,
        session_encryption_key: NodeIdentity::derive_session_encryption_key(seed_b)?,
        enable_mdns: false,
        enable_listeners: true,
        tcp_port: 0,
        enable_relay_server: false,
        max_connections: 128,
        liveness_interval_secs: 30,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: false,
    }).await?;
    tokio::spawn(service_b.run());
    println!("Node B online: {}", id_b.peer_id);

    // Connect Node B to Anchor
    cmd_tx_b.send(NetworkCommand::AddAddress { peer_id: peer_id_anchor, address: anchor_addr.clone() }).await?;
    cmd_tx_b.send(NetworkCommand::Dial { peer_id: peer_id_anchor, address: Some(anchor_addr) }).await?;
    sleep(Duration::from_secs(2)).await;

    // Trigger Drain
    cmd_tx_b.send(NetworkCommand::FetchMailbox).await?;
    
    println!("Waiting for Node B to drain message...");
    sleep(Duration::from_secs(3)).await;
    
    let messages = DRAINED_MESSAGES.lock();
    println!("Drained messages: {:?}", *messages);
    assert!(messages.contains(&test_message.to_string()), "Node B failed to retrieve offline message");

    // --- Part 2: Yield Verification ---
    println!("\n📊 Starting Yield Verification...");
    
    // Simulate 25 hours of uptime for the Anchor
    tracker_anchor.simulate_uptime(25 * 3600); 
    
    // Record 10MB of relay usage
    let base_bytes = 10 * 1024 * 1024;
    tracker_anchor.record_relay("SomeConsumer", base_bytes);
    
    // Prepare proof
    let provider_pubkey = "AnchorSolanaAddress";
    let (amount, _) = tracker_anchor.prepare_reward_proof(provider_pubkey, "SomeConsumer")
        .expect("Failed to prepare proof");
    
    println!("Base Bytes: {}, Yielded Amount: {}", base_bytes, amount);
    
    // 10MB * 1.2 = 12MB
    let expected_yielded = (base_bytes as f64 * 1.2) as u64;
    assert_eq!(amount, expected_yielded, "Availability Yield multiplier not applied correctly!");
    
    println!("✅ Yield Verification Audit PASSED.");

    Ok(())
}
