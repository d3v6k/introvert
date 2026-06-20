use anyhow::Result;
use introvert::identity::NodeIdentity;
use introvert::network::{NetworkCommand, NetworkConfig, NetworkService, FileTransferProgress, SignalingPayload};
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
use std::io::Write;

static FILE_EVENTS: Lazy<Mutex<Vec<FileTransferProgress>>> = Lazy::new(|| Mutex::new(Vec::new()));

extern "C" fn audit_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 12 {
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        if let Ok(progress) = serde_json::from_slice::<FileTransferProgress>(data_slice) {
            FILE_EVENTS.lock().push(progress);
        }
    }
    introvert::introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_group_file_transfer_reliability() -> Result<()> {
    std::env::set_var("INTROVERT_SKIP_BOOTSTRAP", "1");
    println!("📡 Starting Group File Transfer Audit...");
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
    let downloads_dir = temp_dir_b.path().join("downloads");
    std::fs::create_dir_all(&downloads_dir)?;
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
        downloads_dir: downloads_dir.to_string_lossy().to_string(),
        is_stress_test: false,
    }).await?;
    let peer_id_b = id_b.peer_id;
    tokio::spawn(service_b.run());

    // 4. Connect both to Relay
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: relay_peer_id, address: None }).await?;
    cmd_tx_b.send(NetworkCommand::Dial { peer_id: relay_peer_id, address: None }).await?;
    sleep(Duration::from_secs(3)).await;

    // 5. Node B listens on Relay
    let relay_circuit_b = relay_addr.clone()
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(peer_id_b));
    cmd_tx_b.send(NetworkCommand::ListenOn { address: relay_circuit_b }).await?;
    sleep(Duration::from_secs(2)).await;

    // 6. Node A connects to Node B via Relay
    let relay_b_addr = relay_addr.clone()
        .with(libp2p::multiaddr::Protocol::P2pCircuit)
        .with(libp2p::multiaddr::Protocol::P2p(peer_id_b));
    
    println!("Node A establishing relayed connection to Node B: {}", relay_b_addr);
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: peer_id_b, address: Some(relay_b_addr) }).await?;
    sleep(Duration::from_secs(3)).await;

    // 7. Create a 300KB file to transfer (requires 5 chunks of 64KB)
    let file_path = temp_dir_a.path().join("test_data.bin");
    let mut file = std::fs::File::create(&file_path)?;
    let dummy_data = vec![0x42u8; 300 * 1024]; // 300KB of 0x42
    file.write_all(&dummy_data)?;
    file.sync_all()?;
    println!("Dummy file created of size: {} bytes", dummy_data.len());

    let file_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&dummy_data);
        format!("{:x}", hasher.finalize())
    };

    // 8. Node A registers file seeder for group chat
    println!("Registering seeder on Node A...");
    cmd_tx_a.send(NetworkCommand::RegisterSeeder {
        peer_id: peer_id_a,
        transfer_id: "test_gft_1".to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        file_hash: file_hash.clone(),
        chunk_size: 64 * 1024,
        total_chunks: 5,
        group_id: Some("test_group_id".to_string()),
    }).await?;
    sleep(Duration::from_millis(500)).await;

    // 9. Node B initiates pull (HandleIncomingPayload is what is triggered by start_pull)
    println!("Initiating group pull sequence on Node B...");
    let payload = SignalingPayload::FileTransfer {
        transfer_id: "test_gft_1".to_string(),
        filename: "test_data.bin".to_string(),
        mime_type: "application/octet-stream".to_string(),
        file_hash: file_hash.clone(),
        total_size: dummy_data.len(),
        is_relayed: true,
        sender_peer_id: Some(peer_id_a.to_string()),
        group_id: Some("test_group_id".to_string()),
    };
    cmd_tx_b.send(NetworkCommand::HandleIncomingPayload {
        peer_id: peer_id_a,
        payload,
    }).await?;

    // 10. Wait for completion events on Node B
    println!("Waiting for transfer to complete...");
    let mut success = false;
    for _ in 0..30 {
        sleep(Duration::from_secs(1)).await;
        let events = FILE_EVENTS.lock();
        if let Some(last_event) = events.iter().filter(|e| !e.is_outgoing).last() {
            println!("Progress: {:.2}% (Complete: {}, Verified: {})", last_event.progress * 100.0, last_event.is_complete, last_event.is_verified);
            if last_event.is_complete && last_event.is_verified {
                success = true;
                break;
            }
        }
    }

    assert!(success, "Group file transfer failed to complete or verify successfully");
    println!("✅ Group file transfer completed successfully!");

    Ok(())
}
