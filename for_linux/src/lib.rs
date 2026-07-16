#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod identity;
pub mod storage;
pub mod economy;
pub mod network;
pub mod media;
pub mod fcm;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::ffi::{CStr, CString};
use std::time::Duration;
use libc::c_char;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use std::str::FromStr;
use libp2p::{PeerId, Multiaddr};
use bip39::{Mnemonic, Language};
use solana_sdk::signature::Signer;

use crate::identity::NodeIdentity;
use crate::storage::StorageService;
use crate::network::{FfiNetworkCallback, NetworkCommand, NetworkService};
use crate::economy::RewardTracker;
use crate::economy::solana::SolanaIncentiveEngine;
use serde_json::json;

// --- FFI Types & Callbacks ---

#[derive(Debug)]
#[repr(C)]
pub struct FfiResult {
    pub code: i32,
    pub data: *mut u8,
    pub len: usize,
}

impl FfiResult {
    pub fn success() -> Self {
        Self { code: 0, data: std::ptr::null_mut(), len: 0 }
    }

    pub fn error(code: i32, msg: &str) -> Self {
        let bytes = msg.as_bytes();
        let len = bytes.len();
        let ptr = unsafe { libc::malloc(len) as *mut u8 };
        if !ptr.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len); }
        }
        Self { code, data: ptr, len }
    }

    pub fn binary(data: Vec<u8>) -> Self {
        let len = data.len();
        let ptr = unsafe { libc::malloc(len) as *mut u8 };
        if !ptr.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, len); }
        }
        Self { code: 0, data: ptr, len }
    }
}

// Global Callback Typedefs
pub type FfiCallback = extern "C" fn(FfiResult);
pub type FfiRewardCallback = extern "C" fn(i32, *const c_char);

// --- Global Engine State ---

pub struct Engine {
    pub runtime: Runtime,
    pub identity: Arc<NodeIdentity>,
    pub storage: Arc<StorageService>,
    pub reward_tracker: Arc<RewardTracker>,
    pub solana_client: Arc<SolanaIncentiveEngine>,
    pub reward_engine: Arc<crate::economy::daily_rewards::RbnDailyRewardEngine>,
    pub session_encryption_key: [u8; 32],
    pub network_tx: RwLock<Option<mpsc::Sender<NetworkCommand>>>,
    pub network_callback: RwLock<Option<FfiNetworkCallback>>,
    pub downloads_dir: String,
}

pub static ENGINE: Lazy<RwLock<Option<Engine>>> = Lazy::new(|| RwLock::new(None));

pub static ACTIVE_PEER_COUNT: AtomicUsize = AtomicUsize::new(0);

pub static TEST_CALLBACK: Lazy<RwLock<Option<FfiNetworkCallback>>> = Lazy::new(|| RwLock::new(None));

/// Dispatches an event to the global FFI callback. 
/// The memory pointed to by data_ptr MUST be allocated with libc::malloc 
/// and ownership is transferred to Dart (Dart will call libc::free).
pub fn dispatch_global_event_raw(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if data_ptr.is_null() && data_len > 0 {
        println!("FFI Warning: Null data_ptr for non-zero data_len in event {}", event_type);
        return;
    }
    
    if let Some(callback) = *TEST_CALLBACK.read() {
        callback(event_type, data_ptr, data_len);
        return;
    }
    if let Some(engine) = ENGINE.read().as_ref() {
        if let Some(callback) = *engine.network_callback.read() {
            callback(event_type, data_ptr, data_len);
        } else {
            println!("FFI Warning: No callback registered in engine for event {}", event_type);
        }
    } else {
        println!("FFI Warning: Engine not initialized for event {}", event_type);
    }
}

/// Dispatches an event by copying the data into a fresh libc::malloc'd buffer.
/// Ownership is transferred to Dart.
pub fn dispatch_global_event(event_type: i32, data: &[u8]) {
    let len = data.len();
    if len == 0 {
        dispatch_global_event_raw(event_type, std::ptr::null(), 0);
        return;
    }

    let ptr = unsafe { libc::malloc(len) as *mut u8 };
    if ptr.is_null() {
        eprintln!("FFI Error: libc::malloc failed for event {}", event_type);
        return;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, len);
    }
    dispatch_global_event_raw(event_type, ptr, len);
}

pub fn dispatch_debug_log(msg: &str) {
    dispatch_global_event(99, msg.as_bytes());
}

/// Send handle registration to the treasury daemon via IPC.
pub async fn send_handle_registration_to_treasury(
    handle: &str,
    peer_id: &str,
    claimant_pubkey: &str,
    ipc_secret: &str,
) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:9001/handle/register");
    let client = reqwest::Client::new();
    let resp = client.post(&url)
        .json(&serde_json::json!({
            "handle": handle,
            "peer_id": peer_id,
            "claimant_pubkey": claimant_pubkey,
            "ipc_secret": ipc_secret,
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Treasury returned {}", resp.status()));
    }
    Ok(())
}

// --- Identity & BIP-39 Handlers ---

/// Generates a new 12-word mnemonic.
#[no_mangle]
pub extern "C" fn introvert_generate_mnemonic() -> *mut c_char {
    let mut entropy = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut entropy);
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
    CString::new(mnemonic.to_string()).unwrap().into_raw()
}

/// Converts a mnemonic phrase to a 32-byte master seed.
#[no_mangle]
pub extern "C" fn introvert_mnemonic_to_seed(phrase_ptr: *const c_char) -> FfiResult {
    if phrase_ptr.is_null() { return FfiResult::error(-1, "Null pointer"); }
    let phrase = unsafe { CStr::from_ptr(phrase_ptr).to_string_lossy() };
    
    match Mnemonic::parse_in(Language::English, &*phrase) {
        Ok(mnemonic) => {
            let seed = mnemonic.to_seed("");
            FfiResult::binary(seed[..32].to_vec())
        }
        Err(_) => FfiResult::error(-2, "Invalid mnemonic phrase"),
    }
}

// --- Engine Controls ---

#[no_mangle]
pub extern "C" fn introvert_engine_start(
    seed_ptr: *const u8,
    db_path_ptr: *const c_char,
) -> FfiResult {
    if seed_ptr.is_null() || db_path_ptr.is_null() {
        return FfiResult::error(-1, "Null pointer");
    }

    let seed: &[u8; 32] = unsafe { &*(seed_ptr as *const [u8; 32]) };
    let db_path_str = unsafe { CStr::from_ptr(db_path_ptr).to_string_lossy() };
    
    // 1. Initialize Identity
    let identity = match NodeIdentity::from_seed(*seed) {
        Ok(id) => Arc::new(id),
        Err(_) => return FfiResult::error(-2, "Identity derivation failed"),
    };

    // 2. Initialize Storage
    let storage_key = match NodeIdentity::derive_storage_key(*seed) {
        Ok(key) => key,
        Err(_) => return FfiResult::error(-3, "Storage key derivation failed"),
    };

    let storage = match StorageService::new(db_path_str.as_ref(), &storage_key) {
        Ok(s) => Arc::new(s),
        Err(e) => return FfiResult::error(-4, &format!("Storage initialization failed: {}", e)),
    };

    // 3. Initialize Economy & Solana
    let session_encryption_key = match NodeIdentity::derive_session_encryption_key(*seed) {
        Ok(key) => key,
        Err(_) => return FfiResult::error(-8, "Session encryption key derivation failed"),
    };

    let reward_tracker = Arc::new(RewardTracker::new(Some(Arc::clone(&storage))));

    // Initialize Solana Incentive Engine with Mainnet settings as per Blueprint v4.0
    let solana_client = match SolanaIncentiveEngine::new(
        "https://api.mainnet-beta.solana.com",
        "9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy", // Treasury from Blueprint v4.0
        "https://api.introvert.network/v1/treasury/claim" // Production Treasury Relay
    ) {
        Ok(c) => Arc::new(c),
        Err(_) => return FfiResult::error(-7, "Solana client initialization failed"),
    };

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(_) => return FfiResult::error(-6, "Runtime initialization failed"),
    };

    let mut engine_lock = ENGINE.write();
    let downloads_dir = std::path::Path::new(db_path_str.as_ref())
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/tmp".to_string());

    *engine_lock = Some(Engine {
        runtime,
        identity,
        storage,
        reward_tracker,
        solana_client,
        reward_engine: Arc::new(crate::economy::daily_rewards::RbnDailyRewardEngine::new()),
        session_encryption_key,
        network_tx: RwLock::new(None),
        network_callback: RwLock::new(None),
        downloads_dir,
    });

    FfiResult::success()
}

// --- Networking & FFI Plane ---

#[no_mangle]
pub extern "C" fn introvert_network_start(callback: FfiNetworkCallback) -> FfiResult {
    introvert_network_start_ext(callback, 0, false)
}

#[no_mangle]
pub extern "C" fn introvert_network_start_ext(callback: FfiNetworkCallback, tcp_port: u16, enable_relay_server: bool) -> FfiResult {
    introvert_network_start_production(callback, tcp_port, enable_relay_server, 100_000, 600)
}

#[no_mangle]
pub extern "C" fn introvert_network_start_production(
    callback: FfiNetworkCallback, 
    tcp_port: u16, 
    enable_relay_server: bool,
    max_connections: u32,
    liveness_interval_secs: u64
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    // Update the global callback first
    {
        let mut cb_lock = engine.network_callback.write();
        *cb_lock = Some(callback);
    }

    let keypair = engine.identity.keypair.clone();
    let storage = Arc::clone(&engine.storage);
    let reward_tracker = Arc::clone(&engine.reward_tracker);
    let solana_client = Arc::clone(&engine.solana_client);
    let reward_engine = Arc::clone(&engine.reward_engine);
    let (tx, rx) = mpsc::channel(100);
    let tx_clone = tx.clone();

    let local_static_secret = match NodeIdentity::derive_e2ee_key(engine.identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-14, "E2EE key derivation failed"),
    };

    let session_encryption_key = engine.session_encryption_key;

    {
        let mut tx_lock = engine.network_tx.write();
        *tx_lock = Some(tx);
    }

    let downloads_dir = engine.downloads_dir.clone();

    engine.runtime.spawn(async move {
        dispatch_debug_log("Starting NetworkService initialization...");
        match NetworkService::new(
            keypair,
            callback,
            rx,
            tx_clone,
            storage,
            reward_tracker,
            solana_client,
            local_static_secret,
            session_encryption_key,
            true,
            true,
            tcp_port,
            enable_relay_server,
            max_connections,
            liveness_interval_secs,
            downloads_dir,
            false,
            solana_sdk::pubkey::Pubkey::default(),
            reward_engine,
        ).await {
            Ok(service) => {
                dispatch_debug_log("NetworkService initialized. Running swarm...");
                service.run().await;
            }
            Err(e) => {
                let err_msg = format!("Failed to start network service: {}", e);
                eprintln!("{}", err_msg);
                dispatch_debug_log(&err_msg);
            }
        }
    });
    FfiResult::success()
}

/// Starts real-time economy monitoring and pushes updates to Flutter via Event Type 9.
#[no_mangle]
pub extern "C" fn introvert_economy_start_monitoring(callback: FfiNetworkCallback) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    // Register the callback
    {
        let mut cb_lock = engine.network_callback.write();
        *cb_lock = Some(callback);
    }

    let tracker = Arc::clone(&engine.reward_tracker);
    let solana = Arc::clone(&engine.solana_client);
    let identity = Arc::clone(&engine.identity);

    engine.runtime.spawn(async move {
        let solana_signing_key = match NodeIdentity::derive_solana_keypair(identity.seed) {
            Ok(k) => k,
            Err(_) => return,
        };
        let my_pubkey = solana_sdk::pubkey::Pubkey::new_from_array(solana_signing_key.verifying_key().to_bytes());
        let treasury_pubkey = solana.get_treasury_pubkey();

        // Immediate Initial Update
        {
            tracker.update_uptime();
            let balance = solana.fetch_balance(&my_pubkey).await.unwrap_or(0);
            let pending = tracker.get_pending_rewards();
            let total_relayed = tracker.get_total_relayed();

            let stats = json!({
                "intr_balance": balance,
                "pending_rewards": pending,
                "total_relayed": total_relayed,
                "sol_address": my_pubkey.to_string(), 
                "treasury_address": treasury_pubkey.to_string(),
                "token_name": "INTR"
            });

            if let Ok(stats_str) = serde_json::to_string(&stats) {
                dispatch_global_event(9, stats_str.as_bytes());
            }
        }

        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;

            tracker.update_uptime();
            let balance = solana.fetch_balance(&my_pubkey).await.unwrap_or(0);
            let pending = tracker.get_pending_rewards();
            let total_relayed = tracker.get_total_relayed();

            let stats = json!({
                "intr_balance": balance,
                "pending_rewards": pending,
                "total_relayed": total_relayed,
                "sol_address": my_pubkey.to_string(), 
                "treasury_address": treasury_pubkey.to_string(),
                "token_name": "INTR"
            });

            if let Ok(stats_str) = serde_json::to_string(&stats) {
                dispatch_global_event(9, stats_str.as_bytes());
            }
        }
    });

    FfiResult::success()
}

/// Asynchronously claims rewards by generating proofs for all eligible consumers and submitting them to Solana.
#[no_mangle]
pub extern "C" fn introvert_claim_rewards_async(callback: FfiRewardCallback) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let tracker = Arc::clone(&engine.reward_tracker);
    let solana = Arc::clone(&engine.solana_client);
    let identity = Arc::clone(&engine.identity);

    engine.runtime.spawn(async move {
        let solana_signing_key = match NodeIdentity::derive_solana_keypair(identity.seed) {
            Ok(key) => key,
            Err(e) => {
                let err_msg = CString::new(format!("Key derivation failed: {}", e)).unwrap();
                callback(-4, err_msg.into_raw());
                return;
            }
        };
        
        let user_keypair = solana_sdk::signature::Keypair::new_from_array(solana_signing_key.to_bytes());

        let provider_pubkey = user_keypair.pubkey().to_string();
        let consumers = tracker.get_pending_consumers();
        let mut claim_count = 0;

        for consumer_id in consumers {
            if let Some((amount, proof)) = tracker.prepare_reward_proof(&provider_pubkey, &consumer_id) {
                match solana.submit_reward_claim(&user_keypair, &proof).await {
                    Ok(sig) => {
                        tracker.commit_reward_claim(&consumer_id, amount);
                        let sig_msg = CString::new(sig).unwrap();
                        callback(0, sig_msg.into_raw());
                        claim_count += 1;
                    }
                    Err(e) => {
                        let err_msg = CString::new(format!("Claim error for {}: {}", consumer_id, e)).unwrap();
                        callback(-2, err_msg.into_raw());
                    }
                }
            }
        }

        if claim_count == 0 {
            let err_msg = CString::new("No rewards eligible for claim (check threshold/cooldown)").unwrap();
            callback(-3, err_msg.into_raw());
        }
    });

    FfiResult::success()
}

/// Triggers a request to connected Anchors to drain any pending mailbox messages.
#[no_mangle]
pub extern "C" fn introvert_network_fetch_mailbox() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::FetchMailbox).await;
    });

    FfiResult::success()
}

/// Initiates a file transfer to a remote peer.
#[no_mangle]
pub extern "C" fn introvert_network_force_refresh() -> FfiResult {
    let engine_lock = ENGINE.read();
    if let Some(engine) = engine_lock.as_ref() {
        if let Some(ref tx) = *engine.network_tx.read() {
            let tx_clone = tx.clone();
            engine.runtime.spawn(async move {
                let _ = tx_clone.send(NetworkCommand::ForceMeshRefresh).await;
            });
            return FfiResult::success();
        }
    }
    FfiResult::error(-1, "Network not started")
}

#[no_mangle]
pub extern "C" fn introvert_network_get_active_peer_count() -> i32 {
    ACTIVE_PEER_COUNT.load(std::sync::atomic::Ordering::Relaxed) as i32
}

#[no_mangle]
pub extern "C" fn introvert_network_send_file(
    peer_id_ptr: *const c_char,
    file_path_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || file_path_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let file_path = unsafe { CStr::from_ptr(file_path_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::SendFile { peer_id, file_path, group_id: None, transfer_id: None }).await;
    });

    FfiResult::success()
}

/// Explicitly dials a remote peer.
#[no_mangle]
pub extern "C" fn introvert_network_dial(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::Dial { peer_id, address: None }).await;
    });

    FfiResult::success()
}

/// Initiates a Noise IK handshake to establish an Application-Layer E2EE session.
#[no_mangle]
pub extern "C" fn introvert_network_establish_secure_session(
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id }).await;
    });

    FfiResult::success()
}

/// Sends a signaling message to a remote peer via the libp2p plane.
#[no_mangle]
pub extern "C" fn introvert_network_send_message(
    peer_id_ptr: *const c_char,
    msg_ptr: *const c_char,
    callback: FfiCallback,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || msg_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let message = unsafe { CStr::from_ptr(msg_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        match tx.send(NetworkCommand::SendSignaling { peer_id, msg_id: String::new(), message, reply_to: None }).await {
            Ok(_) => {
                callback(FfiResult::success());
            }
            Err(e) => {
                callback(FfiResult::error(-1, &format!("Command error: {}", e)));
            }
        }
    });

    FfiResult::success()
}

/// Initiates a WebRTC data channel connection with a remote peer.
#[no_mangle]
pub extern "C" fn introvert_network_initiate_webrtc(
    peer_id_ptr: *const c_char,
    callback: FfiCallback,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        match tx.send(NetworkCommand::InitiateWebRtc { peer_id, media_type: 3 }).await {
            Ok(_) => {
                callback(FfiResult::success());
            }
            Err(e) => {
                callback(FfiResult::error(-1, &format!("Command error: {}", e)));
            }
        }
    });

    FfiResult::success()
}

/// Manually adds a peer address to the Kademlia routing table for bootstrapping.
#[no_mangle]
pub extern "C" fn introvert_network_add_address(
    peer_id_ptr: *const c_char,
    address_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || address_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let address_str = unsafe { CStr::from_ptr(address_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let address = match Multiaddr::from_str(&address_str) {
        Ok(addr) => addr,
        Err(_) => return FfiResult::error(-14, "Invalid Multiaddr"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::AddAddress { peer_id, address }).await;
    });

    FfiResult::success()
}

/// Retrieves the local profile as a JSON-encoded binary blob.
#[no_mangle]
pub extern "C" fn introvert_storage_get_profile() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_profile() {
        Ok(Some((name, avatar, _handle, _privacy, _tier))) => {
            let json = json!({ "name": name, "avatar": avatar }).to_string();
            FfiResult::binary(json.into_bytes())
        }
        Ok(None) => FfiResult::binary(b"{}".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Sets the local user profile.
#[no_mangle]
pub extern "C" fn introvert_storage_set_profile(
    name_ptr: *const c_char,
    avatar_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let name = if name_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(name_ptr).to_str().unwrap_or_default() }) };
    let avatar = if avatar_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(avatar_ptr).to_str().unwrap_or_default() }) };

    match engine.storage.set_profile(name, None, avatar, 0) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Retrieves all verified contacts from storage as a JSON-encoded binary blob.
#[no_mangle]
pub extern "C" fn introvert_storage_get_contacts() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_all_contacts() {
        Ok(contacts) => {
            let mapped_contacts: Vec<serde_json::Value> = contacts.into_iter().map(|c| {
                json!({
                    "peer_id": c.peer_id,
                    "static_key": c.static_key,
                    "solana_address": c.solana_address,
                    "global_name": c.global_name,
                    "alias": c.local_alias, // UI expects 'alias'
                    "avatar": c.avatar_base64, // UI expects 'avatar'
                    "is_anchor_capable": c.is_anchor_capable,
                })
            }).collect();
            let json = serde_json::to_string(&mapped_contacts).unwrap_or_default();
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Deletes a verified contact from storage.
#[no_mangle]
pub extern "C" fn introvert_storage_delete_contact(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy() };

    match engine.storage.delete_contact(&peer_id) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Delete error: {}", e)),
    }
}

/// Deletes all verified contacts from storage.
#[no_mangle]
pub extern "C" fn introvert_storage_clear_contacts() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.clear_all_contacts() {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Clear error: {}", e)),
    }
}

/// Upgrades a WebRTC connection to support native media (Voice/Video) streams.
/// media_type: 0 = Audio, 1 = Video, 2 = Both.
#[no_mangle]
pub extern "C" fn introvert_network_start_media_stream(
    peer_id_ptr: *const c_char,
    media_type: u8,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::StartMediaStream { peer_id, media_type }).await;
    });

    FfiResult::success()
}

/// Asynchronously persists a message using a non-blocking spawn_blocking task.
#[no_mangle]
pub extern "C" fn introvert_store_message_async(
    peer_id_ptr: *const c_char,
    msg_ptr: *const c_char,
    is_me: bool,
    callback: FfiCallback,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || msg_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let msg = unsafe { CStr::from_ptr(msg_ptr).to_string_lossy().into_owned() };

    let storage = Arc::clone(&engine.storage);

    engine.runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            storage.store_message(&peer_id, &msg, is_me)
        }).await;

        match result {
            Ok(inner_res) => match inner_res {
                Ok(_) => {
                    callback(FfiResult::success());
                }
                Err(e) => {
                    callback(FfiResult::error(-1, &format!("Storage error: {}", e)));
                }
            },
            Err(e) => {
                callback(FfiResult::error(-2, &format!("Runtime error: {}", e)));
            }
        }
    });

    FfiResult::success()
}

/// Retrieves all messages for a specific peer. Result is a JSON array of message objects.
#[no_mangle]
pub extern "C" fn introvert_storage_get_messages(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.get_messages_for_peer(&peer_id) {
        Ok(messages) => {
            println!("FFI: get_messages for peer {} returned {} rows", peer_id, messages.len());
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(content, timestamp, is_me, _status, _msg_id, _reply_to)| {
                // Convert SQLite timestamp (YYYY-MM-DD HH:MM:SS) to ISO 8601 (YYYY-MM-DDTHH:MM:SS)
                let iso_timestamp = timestamp.replace(" ", "T");
                serde_json::json!({
                    "content": content,
                    "timestamp": iso_timestamp,
                    "is_me": is_me
                })
            }).collect();

            let json = serde_json::to_string(&json_messages).unwrap_or_else(|_| "[]".to_string());
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}
/// Initiates a Magic Wormhole invite session using the global network callback.
#[no_mangle]
pub extern "C" fn introvert_wormhole_start() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let identity = Arc::clone(&engine.identity);
    let storage = Arc::clone(&engine.storage);
    
    // Construct our SovereignIdentity
    let local_static_secret = match NodeIdentity::derive_e2ee_key(identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-14, "E2EE key derivation failed"),
    };
    let local_static_public = x25519_dalek::PublicKey::from(&local_static_secret);
    
    let solana_signing_key = match NodeIdentity::derive_solana_keypair(identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-15, "Solana key derivation failed"),
    };
    let solana_address = solana_sdk::pubkey::Pubkey::new_from_array(solana_signing_key.verifying_key().to_bytes()).to_string();

    let (local_name, local_avatar, ..) = storage.get_profile().unwrap_or(None).unwrap_or((None, None, None, 0, 0));

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true,
        retention_seconds: 86400,
        handle: None,
        prestige_tier: None,
    };

    engine.runtime.spawn(async move {
        match crate::network::wormhole::create_invite(my_identity).await {
            Ok((code, handshake_future)) => {
                // Emit the code to the UI (Event Type 6)
                dispatch_global_event(6, code.as_bytes());
                
                // Wait for the peer to connect and exchange identity
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity, false, false);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        dispatch_global_event(7, peer_identity.peer_id.as_bytes());
                    }
                    Err(e) => {
                        eprintln!("Wormhole handshake failed: {}", e);
                        dispatch_global_event(6, format!("ERROR:HANDSHAKE_FAILED:{}", e).as_bytes());
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to create Wormhole invite: {}", e);
                dispatch_global_event(6, format!("ERROR:CREATE_FAILED:{}", e).as_bytes());
            }
        }
    });

    FfiResult::success()
}

/// Joins a Magic Wormhole session using the global network callback.
#[no_mangle]
pub extern "C" fn introvert_wormhole_join(code_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if code_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let code = unsafe { CStr::from_ptr(code_ptr).to_string_lossy().into_owned() };

    let identity = Arc::clone(&engine.identity);
    let storage = Arc::clone(&engine.storage);
    
    // Construct our SovereignIdentity
    let local_static_secret = match NodeIdentity::derive_e2ee_key(identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-14, "E2EE key derivation failed"),
    };
    let local_static_public = x25519_dalek::PublicKey::from(&local_static_secret);
    
    let solana_signing_key = match NodeIdentity::derive_solana_keypair(identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-15, "Solana key derivation failed"),
    };
    let solana_address = solana_sdk::pubkey::Pubkey::new_from_array(solana_signing_key.verifying_key().to_bytes()).to_string();

    let (local_name, local_avatar, ..) = storage.get_profile().unwrap_or(None).unwrap_or((None, None, None, 0, 0));

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true,
        retention_seconds: 86400,
        handle: None,
        prestige_tier: None,
    };

    engine.runtime.spawn(async move {
        match crate::network::wormhole::accept_invite(code, my_identity).await {
            Ok(handshake_future) => {
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity, false, false);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        dispatch_global_event(7, peer_identity.peer_id.as_bytes());
                    }
                    Err(e) => {
                        eprintln!("Wormhole join handshake failed: {}", e);
                        dispatch_global_event(6, format!("ERROR:JOIN_HANDSHAKE_FAILED:{}", e).as_bytes());
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to join Wormhole session: {}", e);
                dispatch_global_event(6, format!("ERROR:JOIN_FAILED:{}", e).as_bytes());
            }
        }
    });

    FfiResult::success()
}

/// Closes an active WebRTC connection for a given peer.
#[no_mangle]
pub extern "C" fn introvert_webrtc_close_connection(
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::CloseWebRtc { peer_id }).await;
    });

    FfiResult::success()
}

/// Triggers a WebRTC renegotiation (re-offer/re-answer) for an active peer session.
#[no_mangle]
pub extern "C" fn introvert_webrtc_renegotiate(
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::RenegotiateWebRtc { peer_id }).await;
        dispatch_global_event(0, b"renegotiation_started");
    });

    FfiResult::success()
}

/// Sets the node's anchor capability. Enabling this makes the node a relay/mailbox provider.
#[no_mangle]
pub extern "C" fn introvert_network_set_anchor_mode(enabled: bool) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::UpdateAnchorStatus { enabled }).await;
    });

    FfiResult::success()
}

#[no_mangle]
pub extern "C" fn introvert_get_peer_id() -> *mut c_char {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        let peer_id = engine.identity.peer_id.to_string();
        CString::new(peer_id).unwrap().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn introvert_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { let _ = CString::from_raw(s); }
    }
}

/// Reclaims leaked binary memory once Dart has finished copying it.
#[no_mangle]
pub extern "C" fn introvert_free_binary(ptr: *mut u8, _len: usize) {
    if !ptr.is_null() {
        unsafe {
            libc::free(ptr as *mut libc::c_void);
        }
    }
}

/// Specialized finalizer for Dart's NativeFinalizer.
#[no_mangle]
pub extern "C" fn introvert_free_binary_finalizer(ptr: *mut libc::c_void) {
    if !ptr.is_null() {
        unsafe {
            libc::free(ptr);
        }
    }
}

#[no_mangle]
pub extern "C" fn introvert_engine_stop() -> FfiResult {
    let mut lock = ENGINE.write();
    if let Some(engine) = lock.take() {
        engine.runtime.shutdown_background();
        FfiResult::success()
    } else {
        FfiResult::error(-1, "Engine not started")
    }
}
