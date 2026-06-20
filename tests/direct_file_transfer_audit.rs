use anyhow::Result;
use introvert::identity::NodeIdentity;
use introvert::network::{NetworkCommand, NetworkConfig, NetworkService, FileTransferProgress};
use introvert::storage::StorageService;
use introvert::economy::RewardTracker;
use libp2p::{PeerId, futures::StreamExt};
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
async fn test_direct_file_transfer_reliability() -> Result<()> {
    std::env::set_var("INTROVERT_SKIP_BOOTSTRAP", "1");
    println!("📡 Starting Direct File Transfer Audit...");
    *introvert::TEST_CALLBACK.write() = Some(audit_callback);

    // 1. Setup Node A (Sender)
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
    let _peer_id_a = id_a.peer_id;
    tokio::spawn(service_a.run());

    // 2. Setup Node B (Receiver)
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

    // 3. Start listener on Node B
    println!("Node B listening on local TCP port 12052...");
    let direct_addr_b = "/ip4/127.0.0.1/tcp/12052".parse()?;
    cmd_tx_b.send(NetworkCommand::ListenOn { address: direct_addr_b }).await?;
    sleep(Duration::from_secs(2)).await;

    // 4. Connect Node A to Node B directly
    let connect_addr = "/ip4/127.0.0.1/tcp/12052/p2p/".to_string() + &peer_id_b.to_string();
    println!("Node A dialing Node B directly: {}", connect_addr);
    cmd_tx_a.send(NetworkCommand::Dial { peer_id: peer_id_b, address: Some(connect_addr.parse()?) }).await?;
    sleep(Duration::from_secs(3)).await;

    // 5. Create a 1MB file to transfer (requires 4 chunks of 256KB)
    let file_path = temp_dir_a.path().join("test_data.bin");
    let mut file = std::fs::File::create(&file_path)?;
    let dummy_data = vec![0x42u8; 1024 * 1024]; // 1MB
    file.write_all(&dummy_data)?;
    file.sync_all()?;
    println!("Dummy file created of size: {} bytes", dummy_data.len());

    // 6. Initiate File Transfer from Node A to Node B
    println!("Initiating direct file transfer...");
    cmd_tx_a.send(NetworkCommand::SendFile { 
        peer_id: peer_id_b, 
        file_path: file_path.to_string_lossy().to_string(),
        group_id: None,
        transfer_id: None,
    }).await?;

    // 7. Wait for completion events on Node B
    println!("Waiting for transfer to complete...");
    let mut success = false;
    for _ in 0..15 {
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

    assert!(success, "Direct file transfer failed to complete or verify successfully");
    println!("✅ Direct file transfer completed successfully!");

    Ok(())
}
