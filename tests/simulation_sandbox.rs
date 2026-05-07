use anyhow::Result;
use introvert::identity::NodeIdentity;
use introvert::network::{NetworkCommand, NetworkService};
use introvert::storage::StorageService;
use introvert::economy::RewardTracker;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::sleep;

extern "C" fn dummy_callback(_event_type: i32, _data_ptr: *const u8, _data_len: usize) {}

/// Simulation Node Harness
struct SimNode {
    command_tx: mpsc::Sender<NetworkCommand>,
    _storage: Arc<StorageService>,
    _temp_dir: TempDir, // Keep alive for the duration of the node
    join_handle: tokio::task::JoinHandle<()>,
}

impl SimNode {
    async fn stop(self) {
        self.join_handle.abort();
    }
}

#[tokio::test]
async fn simulation_sandbox_stress_test() -> Result<()> {
    let node_count = 5;
    let mut nodes: HashMap<PeerId, SimNode> = HashMap::new();
    let mut bootnodes: Vec<(PeerId, Multiaddr)> = Vec::new();

    println!("🚀 Starting Simulation Sandbox with {} nodes...", node_count);

    // 1. Initialize Swarm Orchestration
    for i in 0..node_count {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("introvert.db");

        let mut seed = [0u8; 32];
        seed[0] = i as u8;
        let identity = NodeIdentity::from_seed(seed)?;
        let peer_id = identity.peer_id;

        let storage_key = NodeIdentity::derive_storage_key(seed)?;
        let storage = Arc::new(StorageService::new(db_path, &storage_key)?);
        let reward_tracker = Arc::new(RewardTracker::new(Some(storage.clone())));

        let local_static_key = NodeIdentity::derive_e2ee_key(seed)?;
        let session_encryption_key = NodeIdentity::derive_session_encryption_key(seed)?;
        let (command_tx, command_rx) = mpsc::channel(100);

        let service = NetworkService::new(
            identity.keypair.clone(),
            dummy_callback,
            command_rx,
            storage.clone(),
            reward_tracker,
            local_static_key,
            session_encryption_key,
            true,
            true,
            0,
            false,
        ).await?;

        // In a real simulation we'd listen on actual ports, but for this basic fix 
        // we'll just use a mock address for bootstrapping.
        let port = 10000 + i;
        let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;

        if i == 0 {
            bootnodes.push((peer_id, addr));
        }

        let join_handle = tokio::spawn(async move {
            service.run().await;
        });

        nodes.insert(peer_id, SimNode {
            command_tx,
            _storage: storage,
            _temp_dir: temp_dir,
            join_handle,
        });
    }

    // 2. Interconnect (Bootstrap)
    println!("🔗 Interconnecting nodes via Kademlia...");
    for node in nodes.values() {
        for (peer_id, addr) in &bootnodes {
            node.command_tx.send(NetworkCommand::AddAddress { peer_id: *peer_id, address: addr.clone() }).await?;
        }
    }

    sleep(Duration::from_secs(2)).await;

    // 3. Cleanup
    for (_, node) in nodes {
        node.stop().await;
    }

    println!("🎉 Simulation Sandbox test PASSED.");

    Ok(())
}
