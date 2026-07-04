use introvert::network::{NetworkService, NetworkCommand, NetworkConfig, RBN_WS_URL};
use introvert::storage::StorageService;
use libp2p::identity::Keypair;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let num_nodes: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("[StressTester] 🚀 Launching simulation with {} virtual nodes...", num_nodes);
    println!("[StressTester] 🎯 Target RBN: {}", RBN_WS_URL);

    let mut handles = Vec::new();

    for i in 0..num_nodes {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_virtual_node(i).await {
                eprintln!("[Node {}] ❌ Fatal error: {}", i, e);
            }
        });
        handles.push(handle);
        
        // Staggered boot to prevent local OS descriptor exhaustion
        if i % 10 == 0 {
            sleep(Duration::from_millis(500)).await;
        }
    }

    println!("[StressTester] ✅ All {} nodes spawned. Monitoring mesh activity...", num_nodes);

    // Keep the main thread alive
    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

async fn run_virtual_node(index: usize) -> anyhow::Result<()> {
    // 1. Identity & Ephemeral Storage
    let keypair = Keypair::generate_ed25519();
    let storage = Arc::new(StorageService::new_ephemeral()?);
    
    let (tx, rx) = mpsc::channel(100);
    let tx_clone = tx.clone();
    
    // Use a unique static secret for Noise handshakes
    let mut seed = [0u8; 32];
    rand::thread_rng().fill(&mut seed);
    let static_secret = x25519_dalek::StaticSecret::from(seed);

    // 3. Launch Network Service in Stress Test mode
    let service = NetworkService::new(NetworkConfig {
        keypair,
        command_rx: rx,
        command_tx: tx_clone,
        storage: storage.clone(),
        reward_tracker: Arc::new(introvert::economy::RewardTracker::new(Some(storage))),
        solana_client: Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new(
            "http://localhost:8899", 
            "11111111111111111111111111111111",
            "11111111111111111111111111111111"
        )?),
        local_static_secret: static_secret,
        session_encryption_key: [0u8; 32],
        enable_mdns: false,
        enable_listeners: true,
        tcp_port: 0,
        enable_relay_server: false,
        max_connections: 128,
        liveness_interval_secs: 30,
        downloads_dir: "/tmp".to_string(),
        is_stress_test: true,
    }).await?;

    tokio::spawn(async move {
        service.run().await;
    });

    // 4. Randomized Action Loop
    let stress_topic = "introvert_stress_mesh".to_string();
    
    // Initial wait for bootstrap
    sleep(Duration::from_secs(5 + (index % 10) as u64)).await;

    loop {
        let action = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..100)
        };
        
        if action < 10 {
            // 10% chance: Send synthetic encrypted group message
            let msg = format!("Synthetic message from Node {}: test", index);
            let _ = tx.send(NetworkCommand::BroadcastGroupMessage {
                group_id: stress_topic.clone(),
                message: msg,
                reply_to: None,
            }).await;
        } else if action < 15 {
            // 5% chance: Announce a file manifest
            let manifest = format!("[FILE]:{{\"transfer_id\":\"stress_test\",\"filename\":\"test.dat\"}}");
            let _ = tx.send(NetworkCommand::BroadcastGroupMessage {
                group_id: stress_topic.clone(),
                message: manifest,
                reply_to: None,
            }).await;
        }

        let jitter = {
            let mut rng = rand::thread_rng();
            rng.gen_range(10..60)
        };
        sleep(Duration::from_secs(jitter)).await;
    }
}
