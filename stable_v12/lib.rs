#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod identity;
pub mod storage;
pub mod economy;
pub mod network;
pub mod media;

use std::sync::Arc;
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
    pub session_encryption_key: [u8; 32],
    pub network_tx: RwLock<Option<mpsc::Sender<NetworkCommand>>>,
    pub network_callback: RwLock<Option<FfiNetworkCallback>>,
    pub is_anchor_mode: RwLock<bool>,
    pub downloads_dir: String,
}

pub static ENGINE: Lazy<RwLock<Option<Engine>>> = Lazy::new(|| RwLock::new(None));

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

/// Derives the libp2p Peer ID and Solana Wallet address from a 32-byte master seed.
#[no_mangle]
pub extern "C" fn introvert_derive_identifiers(
    seed_ptr: *const u8,
) -> FfiResult {
    if seed_ptr.is_null() {
        return FfiResult::error(-1, "Null pointer");
    }

    let seed: &[u8; 32] = unsafe { &*(seed_ptr as *const [u8; 32]) };
    
    // 1. Derive libp2p identity and peer ID
    let identity = match NodeIdentity::from_seed(*seed) {
        Ok(id) => id,
        Err(_) => return FfiResult::error(-2, "Identity derivation failed"),
    };
    let peer_id = identity.peer_id.to_string();

    // 2. Derive Solana keypair and address
    let sol_signing_key = match NodeIdentity::derive_solana_keypair(*seed) {
        Ok(key) => key,
        Err(_) => return FfiResult::error(-3, "Solana key derivation failed"),
    };
    let sol_pubkey = solana_sdk::pubkey::Pubkey::new_from_array(sol_signing_key.verifying_key().to_bytes());
    let solana_address = sol_pubkey.to_string();

    // 3. Serialize as JSON
    let json_res = json!({
        "peer_id": peer_id,
        "solana_address": solana_address,
    });

    match serde_json::to_string(&json_res) {
        Ok(s) => FfiResult::binary(s.into_bytes()),
        Err(_) => FfiResult::error(-4, "JSON serialization failed"),
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
        session_encryption_key,
        network_tx: RwLock::new(None),
        network_callback: RwLock::new(None),
        is_anchor_mode: RwLock::new(false),
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
    introvert_network_start_production(callback, tcp_port, enable_relay_server, 512, 600)
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
    let (tx, rx) = mpsc::channel(1_000);
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

        let usdc_mint = solana_sdk::pubkey::Pubkey::from_str("EPjFW33V15rFU17EwyAF511wCRh34J1GNzmdLgJDPd59").unwrap();

        // Immediate Initial Update
        {
            tracker.update_uptime();
            let balance = solana.fetch_balance(&my_pubkey).await.unwrap_or(0);
            let sol_balance = solana.fetch_sol_balance(&my_pubkey).await.unwrap_or(0);
            let usdc_balance = solana.fetch_token_balance(&my_pubkey, &usdc_mint).await.unwrap_or(0);
            let pending = tracker.get_pending_rewards();
            let total_relayed = tracker.get_total_relayed();

            let stats = json!({
                "intr_balance": balance,
                "sol_balance": sol_balance,
                "usdc_balance": usdc_balance,
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
            let sol_balance = solana.fetch_sol_balance(&my_pubkey).await.unwrap_or(0);
            let usdc_balance = solana.fetch_token_balance(&my_pubkey, &usdc_mint).await.unwrap_or(0);
            let pending = tracker.get_pending_rewards();
            let total_relayed = tracker.get_total_relayed();

            let stats = json!({
                "intr_balance": balance,
                "sol_balance": sol_balance,
                "usdc_balance": usdc_balance,
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
pub extern "C" fn introvert_network_send_file(
    peer_id_ptr: *const c_char,
    file_path_ptr: *const c_char,
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || file_path_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let file_path = unsafe { CStr::from_ptr(file_path_ptr).to_string_lossy().into_owned() };
    let group_id = if !group_id_ptr.is_null() {
        let gid = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
        if gid.is_empty() { None } else { Some(gid) }
    } else {
        None
    };
    
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
        let _ = tx.send(crate::network::NetworkCommand::SendFile { peer_id, file_path, group_id }).await;
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

    // Privacy gate: check if peer is a contact (verified via dual handshake)
    match engine.storage.get_contact(&peer_id_str) {
        Ok(Some(_)) => {},
        _ => return FfiResult::error(-14, "Privacy Restriction: recipient is not in your contacts list. Handshake required."),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let msg_id = format!("m_{}_{}", peer_id_str, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let mid_cb = msg_id.clone();
        match tx.send(NetworkCommand::SendSignaling { peer_id, msg_id, message }).await {
            Ok(_) => {
                callback(FfiResult::binary(mid_cb.into_bytes()));
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
        match tx.send(NetworkCommand::InitiateWebRtc { peer_id }).await {
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
        Ok(Some((name, avatar))) => {
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

    match engine.storage.set_profile(name, avatar) {
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

/// Sends a real-time acknowledgement (receipt) for a specific message.
/// status: 1 = Delivered, 2 = Read.
#[no_mangle]
pub extern "C" fn introvert_network_send_acknowledgement(
    peer_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
    status: u8,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || msg_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    
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
        let _ = tx.send(NetworkCommand::SendAcknowledgement { peer_id, msg_id, status }).await;
    });

    FfiResult::success()
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

/// Updates the status of a specific message in storage.
#[no_mangle]
pub extern "C" fn introvert_storage_update_message_status(
    msg_id_ptr: *const c_char,
    status: u8,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if msg_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.update_message_status(&msg_id, status) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
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
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(content, timestamp, is_me, status, msg_id)| {
                // Convert SQLite timestamp (YYYY-MM-DD HH:MM:SS) to ISO 8601 (YYYY-MM-DDTHH:MM:SS)
                let iso_timestamp = timestamp.replace(" ", "T");
                serde_json::json!({
                    "content": content,
                    "timestamp": iso_timestamp,
                    "is_me": is_me,
                    "status": status,
                    "msg_id": msg_id
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

    let (local_name, local_avatar) = storage.get_profile().unwrap_or(None).unwrap_or((None, None));

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true, 
    };

    engine.runtime.spawn(async move {
        dispatch_debug_log("Wormhole: Connecting to mailbox relay...");
        
        let invite_result = tokio::time::timeout(
            Duration::from_secs(30),
            crate::network::wormhole::create_invite(my_identity)
        ).await;

        match invite_result {
            Ok(Ok((code, handshake_future))) => {
                dispatch_debug_log(&format!("Wormhole: Code generated: {}", code));
                // Emit the code to the UI (Event Type 6)
                dispatch_global_event(6, code.as_bytes());
                
                // Wait for the peer to connect and exchange identity
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        dispatch_global_event(7, peer_identity.peer_id.as_bytes());
                    }
                    Err(e) => {
                        eprintln!("Wormhole handshake failed: {}", e);
                        dispatch_global_event(6, format!("ERROR:HANDSHAKE_FAILED:{}", e).as_bytes());
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("Failed to create Wormhole invite: {}", e);
                dispatch_global_event(6, format!("ERROR:CREATE_FAILED:{}", e).as_bytes());
            }
            Err(_) => {
                eprintln!("Wormhole invite creation timed out");
                dispatch_global_event(6, "ERROR:TIMEOUT:Mailbox relay unreachable".as_bytes());
            }
        }
    });

    FfiResult::success()
}

/// Deletes all messages associated with a peer.
#[no_mangle]
pub extern "C" fn introvert_storage_delete_chat(peer_id_ptr: *const c_char) -> FfiResult {
    if peer_id_ptr.is_null() { return FfiResult::error(-1, "Null peer_id"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy() };

    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.delete_chat(&peer_id) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
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

    let (local_name, local_avatar) = storage.get_profile().unwrap_or(None).unwrap_or((None, None));

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true, 
    };

    engine.runtime.spawn(async move {
        match crate::network::wormhole::accept_invite(code, my_identity).await {
            Ok(handshake_future) => {
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity);
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

    {
        let mut anchor_lock = engine.is_anchor_mode.write();
        *anchor_lock = enabled;
    }

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

/// Returns 1 if Anchor Mode is enabled, 0 otherwise.
#[no_mangle]
pub extern "C" fn introvert_network_get_anchor_mode() -> i32 {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        if *engine.is_anchor_mode.read() { 1 } else { 0 }
    } else {
        0
    }
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

/// Shuts down the engine and permanently deletes the local profile/database.
#[no_mangle]
pub extern "C" fn introvert_nuke_identity(
    db_path_ptr: *const c_char,
) -> FfiResult {
    let mut lock = ENGINE.write();
    if let Some(engine) = lock.take() {
        engine.runtime.shutdown_background();
    }

    if db_path_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let db_path_str = unsafe { CStr::from_ptr(db_path_ptr).to_string_lossy().into_owned() };

    let db_path = std::path::Path::new(&db_path_str);
    if db_path.exists() {
        let _ = std::fs::remove_file(db_path);
    }
    let wal_path = db_path.with_extension("db-wal");
    if wal_path.exists() {
        let _ = std::fs::remove_file(wal_path);
    }
    let shm_path = db_path.with_extension("db-shm");
    if shm_path.exists() {
        let _ = std::fs::remove_file(shm_path);
    }

    FfiResult::success()
}

/// Updates the local alias/name for a sovereign contact.
#[no_mangle]
pub extern "C" fn introvert_storage_update_contact_alias(
    peer_id_ptr: *const c_char,
    alias_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || alias_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }

    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let alias = unsafe { CStr::from_ptr(alias_ptr).to_string_lossy().into_owned() };

    match engine.storage.update_contact_alias(&peer_id, &alias) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Adds a file to the local encrypted Drive metadata store.
#[no_mangle]
pub extern "C" fn introvert_drive_add_file(
    filename_ptr: *const c_char,
    file_hash_ptr: *const c_char,
    mime_type_ptr: *const c_char,
    size: i64,
    local_path_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if filename_ptr.is_null() || file_hash_ptr.is_null() || mime_type_ptr.is_null() || local_path_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let filename = unsafe { CStr::from_ptr(filename_ptr).to_string_lossy().into_owned() };
    let file_hash = unsafe { CStr::from_ptr(file_hash_ptr).to_string_lossy().into_owned() };
    let mime_type = unsafe { CStr::from_ptr(mime_type_ptr).to_string_lossy().into_owned() };
    let local_path = unsafe { CStr::from_ptr(local_path_ptr).to_string_lossy().into_owned() };

    match engine.storage.upsert_drive_file(&filename, &file_hash, &mime_type, size, &local_path) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Retrieves all files currently stored in the local encrypted Drive.
#[no_mangle]
pub extern "C" fn introvert_drive_get_all() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_all_drive_files() {
        Ok(files) => {
            let json = serde_json::to_string(&files).unwrap_or_default();
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Deletes a file from the local encrypted Drive.
#[no_mangle]
pub extern "C" fn introvert_drive_delete(
    file_hash_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if file_hash_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let file_hash = unsafe { CStr::from_ptr(file_hash_ptr).to_string_lossy().into_owned() };

    match engine.storage.delete_drive_file(&file_hash) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Returns the estimated total storage capacity of the mesh network.
#[no_mangle]
pub extern "C" fn introvert_get_mesh_capacity() -> i64 {
    // 1 GB standard sovereign storage capacity allocation
    1 * 1024 * 1024 * 1024
}

/// Returns the disk total space and free space on the given path via pointers.
#[no_mangle]
pub extern "C" fn introvert_get_disk_space(
    path_ptr: *const c_char,
    total_bytes: *mut u64,
    free_bytes: *mut u64,
) -> i32 {
    if path_ptr.is_null() || total_bytes.is_null() || free_bytes.is_null() {
        return -1;
    }
    let path = unsafe { CStr::from_ptr(path_ptr).to_string_lossy() };
    let c_path = match CString::new(path.as_ref()) {
        Ok(c) => c,
        Err(_) => return -2,
    };
    
    unsafe {
        let mut stats: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stats) == 0 {
            *total_bytes = stats.f_blocks as u64 * stats.f_frsize as u64;
            *free_bytes = stats.f_bavail as u64 * stats.f_frsize as u64;
            0
        } else {
            -3
        }
    }
}

/// Creates a new E2EE group.
#[no_mangle]
pub extern "C" fn introvert_group_create(
    name_ptr: *const c_char,
    description_ptr: *const c_char,
    members_json_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if name_ptr.is_null() || description_ptr.is_null() || members_json_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let name = unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() };
    let description = unsafe { CStr::from_ptr(description_ptr).to_string_lossy().into_owned() };
    let members_json_str = unsafe { CStr::from_ptr(members_json_ptr).to_string_lossy().into_owned() };

    let group_id = hex::encode(rand::random::<[u8; 16]>());
    let secret = rand::random::<[u8; 32]>();

    let creator_peer_id = engine.identity.peer_id.to_string();
    let creator_pubkey = engine.identity.keypair.public().encode_protobuf();
    let creator_member = crate::network::GroupMemberMetadata {
        peer_id: creator_peer_id,
        pubkey: creator_pubkey,
        role: crate::network::GroupRole::Creator,
    };

    let mut members = vec![creator_member];

    let initial_peer_ids: Vec<String> = serde_json::from_str(&members_json_str).unwrap_or_default();
    for peer_id_str in initial_peer_ids {
        if let Ok(Some(contact)) = engine.storage.get_contact(&peer_id_str) {
            members.push(crate::network::GroupMemberMetadata {
                peer_id: peer_id_str,
                pubkey: contact.p2p_pubkey,
                role: crate::network::GroupRole::Member,
            });
        }
    }

    let updated_members_json = match serde_json::to_string(&members) {
        Ok(json) => json,
        Err(_) => return FfiResult::error(-1, "Failed to serialize members"),
    };

    if let Err(e) = engine.storage.upsert_group(&group_id, &name, &description, &updated_members_json) {
        return FfiResult::error(-2, &format!("Database error: {}", e));
    }
    if let Err(e) = engine.storage.save_group_secret(&group_id, &secret) {
        return FfiResult::error(-3, &format!("Database error: {}", e));
    }

    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let group_id_clone = group_id.clone();
        let name_clone = name.clone();
        let description_clone = description.clone();
        let my_peer_id = engine.identity.peer_id.to_string();
        let members_clone = members.clone();

        let storage = engine.storage.clone();
        engine.runtime.spawn(async move {
            for m in members_clone {
                if m.peer_id == my_peer_id { continue; }
                if let Ok(pid) = PeerId::from_str(&m.peer_id) {
                    if let Ok(Some(contact)) = storage.get_contact(&m.peer_id) {
                        if let Ok(wrapped) = crate::network::group::GroupManager::wrap_group_secret(&secret, &contact.static_key) {
                            let invite = crate::network::SignalingPayload::GroupInvite {
                                group_id: group_id_clone.clone(),
                                name: name_clone.clone(),
                                description: description_clone.clone(),
                                inviter_peer_id: my_peer_id.clone(),
                                group_secret_wrapped: wrapped,
                                members: members.clone(),
                            };
                            let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: invite }).await;
                        }
                    }
                }
            }
        });
    }

    FfiResult::binary(group_id.into_bytes())
}

/// Sends a group message.
#[no_mangle]
pub extern "C" fn introvert_group_send_message(
    group_id_ptr: *const c_char,
    msg_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || msg_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let message = unsafe { CStr::from_ptr(msg_ptr).to_string_lossy().into_owned() };

    let group_secret_vec = match engine.storage.load_group_secret(&group_id) {
        Ok(Some(s)) => s,
        _ => return FfiResult::error(-1, "Group secret not found"),
    };
    if group_secret_vec.len() != 32 {
        return FfiResult::error(-2, "Invalid group secret length");
    }
    let mut group_secret = [0u8; 32];
    group_secret.copy_from_slice(&group_secret_vec);

    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
    use rand::RngCore;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_secret));
    
    let encrypted = match cipher.encrypt(Nonce::from_slice(&nonce_bytes), message.as_bytes()) {
        Ok(c) => c,
        Err(e) => return FfiResult::error(-3, &format!("Encryption error: {:?}", e)),
    };
    let mut content_encrypted = nonce_bytes.to_vec();
    content_encrypted.extend(encrypted);

    let mut msg_id = format!("gm_{}_{}", group_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    if message.starts_with("[FILE]:") {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&message[7..]) {
            if let Some(tid) = meta.get("transfer_id").and_then(|v| v.as_str()) {
                msg_id = tid.to_string();
            }
        }
    }
    let action = crate::network::GroupAction::Message { content_encrypted, msg_id: msg_id.clone() };
    let signed = match crate::network::group::GroupManager::sign_action(group_id.clone(), action, &engine.identity.keypair) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
    };

    let my_peer_id = engine.identity.peer_id.to_string();
    if let Err(e) = engine.storage.store_group_message(&group_id, &my_peer_id, &msg_id, &message) {
        return FfiResult::error(-5, &format!("Database error: {}", e));
    }

    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let payload = crate::network::SignalingPayload::GroupAction(signed);
        let group_id_clone = group_id.clone();
        let my_peer_id_clone = my_peer_id.clone();
        let storage = engine.storage.clone();

        engine.runtime.spawn(async move {
            if let Ok(Some(members_json)) = storage.get_group_members(&group_id_clone) {
                let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                for m in members {
                    if m.peer_id == my_peer_id_clone { continue; }
                    if let Ok(pid) = PeerId::from_str(&m.peer_id) {
                        let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                    }
                }
            }
        });
    }

    FfiResult::success()
}

/// Retrieves all groups.
#[no_mangle]
pub extern "C" fn introvert_group_get_all() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_all_groups() {
        Ok(groups) => {
            let mut groups_json = Vec::new();
            for (gid, name, members, desc) in groups {
                groups_json.push(vec![
                    serde_json::Value::String(gid),
                    serde_json::Value::String(name),
                    serde_json::Value::String(members),
                    serde_json::Value::String(desc),
                ]);
            }
            let json_str = match serde_json::to_string(&groups_json) {
                Ok(s) => s,
                Err(_) => return FfiResult::error(-1, "Serialization error"),
            };
            FfiResult::binary(json_str.into_bytes())
        }
        Err(e) => FfiResult::error(-2, &format!("Database error: {}", e)),
    }
}

/// Retrieves all group messages.
#[no_mangle]
pub extern "C" fn introvert_group_get_messages(
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let my_peer_id = engine.identity.peer_id.to_string();

    match engine.storage.get_group_messages(&group_id) {
        Ok(msgs) => {
            let mut msgs_json = Vec::new();
            for (sender_id, _msg_id, content, timestamp) in msgs {
                let sender_name = if sender_id == my_peer_id {
                    "me".to_string()
                } else if let Ok(Some(contact)) = engine.storage.get_contact(&sender_id) {
                    contact.local_alias.clone().or(contact.global_name.clone()).unwrap_or_else(|| sender_id.clone())
                } else {
                    sender_id.clone()
                };

                msgs_json.push(vec![
                    serde_json::Value::String(sender_id),
                    serde_json::Value::String(sender_name),
                    serde_json::Value::String(content),
                    serde_json::Value::String(timestamp),
                ]);
            }
            let json_str = match serde_json::to_string(&msgs_json) {
                Ok(s) => s,
                Err(_) => return FfiResult::error(-1, "Serialization error"),
            };
            FfiResult::binary(json_str.into_bytes())
        }
        Err(e) => FfiResult::error(-2, &format!("Database error: {}", e)),
    }
}

/// Adds a member to a group.
#[no_mangle]
pub extern "C" fn introvert_group_add_member(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || peer_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::AddGroupMember { group_id, peer_id: peer_id_str }).await;
    });

    FfiResult::success()
}

/// Removes a member from a group.
#[no_mangle]
pub extern "C" fn introvert_group_remove_member(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || peer_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::RemoveGroupMember { group_id, peer_id: peer_id_str }).await;
    });

    FfiResult::success()
}

/// Appoints/updates role of a member.
#[no_mangle]
pub extern "C" fn introvert_group_update_role(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
    role_val: i32,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || peer_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let role = match role_val {
        0 => crate::network::GroupRole::Creator,
        1 => crate::network::GroupRole::Admin,
        _ => crate::network::GroupRole::Member,
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::UpdateGroupRole { group_id, peer_id: peer_id_str, role }).await;
    });

    FfiResult::success()
}

/// Publishes the group manifest discovery record.
#[no_mangle]
pub extern "C" fn introvert_group_publish_manifest(
    group_id_ptr: *const c_char,
    code_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || code_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let code = unsafe { CStr::from_ptr(code_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::PublishGroupManifest { group_id, code }).await;
    });

    FfiResult::success()
}

/// Joins a group by discovery code.
#[no_mangle]
pub extern "C" fn introvert_group_join_by_code(
    code_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if code_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let code = unsafe { CStr::from_ptr(code_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::JoinMeshByCode { code }).await;
    });

    FfiResult::success()
}

/// Deletes a group (Creator-only).
#[no_mangle]
pub extern "C" fn introvert_group_delete(
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let my_peer_id = engine.identity.peer_id.to_string();

    if let Ok(Some(group_info)) = engine.storage.get_group(&group_id) {
        let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
        let is_creator = members.iter().any(|m| m.peer_id == my_peer_id && m.role == crate::network::GroupRole::Creator);
        if !is_creator {
            return FfiResult::error(-12, "Permission denied: Only the main group creator can delete the group");
        }

        let tx_lock = engine.network_tx.read();
        if let Some(tx) = tx_lock.as_ref() {
            let tx = tx.clone();
            let action = crate::network::GroupAction::DeleteGroup;
            if let Ok(signed) = crate::network::group::GroupManager::sign_action(group_id.clone(), action, &engine.identity.keypair) {
                let payload = crate::network::SignalingPayload::GroupAction(signed);
                let my_peer_id_clone = my_peer_id.clone();
                
                engine.runtime.spawn(async move {
                    for m in members {
                        if m.peer_id == my_peer_id_clone { continue; }
                        if let Ok(pid) = PeerId::from_str(&m.peer_id) {
                            let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                        }
                    }
                });
            }
        }
    }

    match engine.storage.delete_group(&group_id) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Retrieves all pending group invitations.
#[no_mangle]
pub extern "C" fn introvert_group_get_pending_invites() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_pending_invites() {
        Ok(invites) => {
            let mut invites_json = Vec::new();
            for inv in invites {
                invites_json.push(serde_json::json!({
                    "group_id": inv.group_id,
                    "name": inv.name,
                    "description": inv.description,
                    "inviter_peer_id": inv.inviter_peer_id,
                    "members_json": inv.members_json,
                }));
            }
            let json_str = match serde_json::to_string(&invites_json) {
                Ok(s) => s,
                Err(_) => return FfiResult::error(-1, "Serialization error"),
            };
            FfiResult::binary(json_str.into_bytes())
        }
        Err(e) => FfiResult::error(-2, &format!("Database error: {}", e)),
    }
}

/// Accepts a pending group invitation.
#[no_mangle]
pub extern "C" fn introvert_group_accept_invite(
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::AcceptGroupInvite { group_id }).await;
    });

    FfiResult::success()
}

/// Declines a pending group invitation.
#[no_mangle]
pub extern "C" fn introvert_group_decline_invite(
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::DeclineGroupInvite { group_id }).await;
    });

    FfiResult::success()
}

/// Registers the local node as a seeder for a file.
#[no_mangle]
pub extern "C" fn introvert_network_register_seeder(
    transfer_id_ptr: *const c_char,
    file_path_ptr: *const c_char,
    file_hash_ptr: *const c_char,
    total_size: i64,
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if transfer_id_ptr.is_null() || file_path_ptr.is_null() || file_hash_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let transfer_id = unsafe { CStr::from_ptr(transfer_id_ptr).to_string_lossy().into_owned() };
    let file_path = unsafe { CStr::from_ptr(file_path_ptr).to_string_lossy().into_owned() };
    let file_hash = unsafe { CStr::from_ptr(file_hash_ptr).to_string_lossy().into_owned() };
    let group_id = if !group_id_ptr.is_null() {
        let gid = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
        if gid.is_empty() { None } else { Some(gid) }
    } else {
        None
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let chunk_size = 256 * 1024;
    let total_chunks = (total_size as f32 / chunk_size as f32).ceil() as u32;
    let local_peer_id = engine.identity.peer_id;

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::RegisterSeeder {
            peer_id: local_peer_id,
            transfer_id,
            file_path,
            file_hash,
            chunk_size,
            total_chunks,
            group_id,
        }).await;
    });

    FfiResult::success()
}

/// Simulates receiving a FileTransfer payload from a peer to initiate download pull.
#[no_mangle]
pub extern "C" fn introvert_network_start_pull(
    peer_id_ptr: *const c_char,
    transfer_id_ptr: *const c_char,
    filename_ptr: *const c_char,
    mime_type_ptr: *const c_char,
    file_hash_ptr: *const c_char,
    total_size: i64,
    is_relayed: bool,
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || transfer_id_ptr.is_null() || filename_ptr.is_null() || mime_type_ptr.is_null() || file_hash_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let transfer_id = unsafe { CStr::from_ptr(transfer_id_ptr).to_string_lossy().into_owned() };
    let filename = unsafe { CStr::from_ptr(filename_ptr).to_string_lossy().into_owned() };
    let mime_type = unsafe { CStr::from_ptr(mime_type_ptr).to_string_lossy().into_owned() };
    let file_hash = unsafe { CStr::from_ptr(file_hash_ptr).to_string_lossy().into_owned() };
    let group_id = if !group_id_ptr.is_null() {
        let s = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    };

    let peer = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let payload = crate::network::SignalingPayload::FileTransfer {
        transfer_id,
        filename,
        mime_type,
        file_hash,
        total_size: total_size as usize,
        is_relayed,
        sender_peer_id: Some(peer_id_str),
        group_id,
    };

    engine.runtime.spawn(async move {
        // Forward signaling directly to ourselves as if received from 'peer'
        let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
    });

    FfiResult::success()
}
