use introvert::network::{NetworkService, NetworkCommand};
use introvert::storage::StorageService;
use libp2p::identity::Keypair;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

extern "C" fn dummy_callback(_t: i32, _d: *const u8, _l: usize) {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let num_nodes: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let rbn_addr = "47.89.252.80";

    println!("[StressTester] 🚀 Launching simulation with {} virtual nodes...", num_nodes);
    println!("[StressTester] 🎯 Target RBN: {}", rbn_addr);

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
    let service = NetworkService::new(
        keypair,
        dummy_callback,
        rx,
        tx_clone,
        storage.clone(),
        Arc::new(introvert::economy::RewardTracker::new(Some(storage.clone()))),
        // Use valid dummy Solana Pubkeys (32-byte base58)
        Arc::new(introvert::economy::solana::SolanaIncentiveEngine::new(
            "http://localhost:8899", 
            "11111111111111111111111111111111", // System Program
            "11111111111111111111111111111111"
        )?),
        static_secret,
        [0u8; 32], // Session key
        false, // No mDNS for mass-simulation
        true, // Enable listeners
        0, // Dynamic port
        false, // No relay server on stress nodes
        128, // Connections per node
        30, // Liveness
        "/tmp".to_string(),
        true, // IS_STRESS_TEST = true
        solana_sdk::pubkey::Pubkey::default(), // Dummy operator pubkey for stress test
        Arc::new(introvert::economy::daily_rewards::RbnDailyRewardEngine::new()), // Dummy reward engine
    ).await?;

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
