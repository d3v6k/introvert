// SAFETY: All extern "C" functions take raw pointers from Dart FFI.
// Each function validates null pointers before dereferencing (see individual functions).
// The clippy lint is overly strict for FFI boundary functions where the caller (Dart)
// is responsible for passing valid pointers from the managed side.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod identity;
pub mod storage;
pub mod economy;
pub mod network;
pub mod media;
pub mod intro_claw;
pub mod embedding;

use std::sync::Arc;
use std::ffi::{CStr, CString};
use std::time::Duration;
use libc::c_char;
use once_cell::sync::Lazy;
use tracing::{error, debug};
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use std::str::FromStr;
use libp2p::{PeerId, Multiaddr};
use bip39::{Mnemonic, Language};
use solana_sdk::signature::Signer;

use crate::identity::NodeIdentity;
use crate::storage::StorageService;
use crate::network::{FfiNetworkCallback, NetworkCommand, NetworkConfig, NetworkService};
use crate::economy::RewardTracker;
use crate::economy::solana::SolanaIncentiveEngine;
use serde_json::json;

// --- FFI Types & Callbacks ---

/// FFI return type for all functions that return data to Dart.
///
/// # Memory Ownership Contract
///
/// - `data` is allocated with `libc::malloc` when `len > 0`
/// - **Dart MUST call `introvert_free_binary(data, len)` after reading the data**
/// - If `code != 0`, `data` may contain an error message (also must be freed)
/// - If `len == 0`, `data` is null and no free is needed
/// - Rust NEVER frees `data` — ownership transfers to the caller
/// - Double-free is prevented by Dart setting its local pointer to null after free
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
        if data.is_empty() {
            return Self { code: 0, data: std::ptr::null_mut(), len: 0 };
        }
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
    pub daily_reward_engine: Option<Arc<crate::economy::daily_rewards::DailyRewardEngine>>,
    pub session_encryption_key: [u8; 32],
    pub network_tx: RwLock<Option<mpsc::Sender<NetworkCommand>>>,
    pub network_callback: RwLock<Option<FfiNetworkCallback>>,
    pub is_anchor_mode: RwLock<bool>,
    pub is_tunnel_mode: RwLock<bool>,
    pub downloads_dir: String,
}

pub static ENGINE: Lazy<RwLock<Option<Engine>>> = Lazy::new(|| RwLock::new(None));

pub static RBN_LATENCIES: Lazy<RwLock<std::collections::HashMap<String, u128>>> = Lazy::new(|| RwLock::new(std::collections::HashMap::new()));
pub static BOOTSTRAP_NODES: Lazy<RwLock<Vec<(String, String)>>> = Lazy::new(|| RwLock::new(Vec::new()));

pub static ACTIVE_PEER_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub static TEST_CALLBACK: Lazy<RwLock<Option<FfiNetworkCallback>>> = Lazy::new(|| RwLock::new(None));

pub static WORMHOLE_TASK: Lazy<parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>> = Lazy::new(|| parking_lot::Mutex::new(None));

/// Dispatches an event to the global FFI callback. 
/// The memory pointed to by data_ptr MUST be allocated with libc::malloc 
/// and ownership is transferred to Dart (Dart will call libc::free).
pub fn dispatch_global_event_raw(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if data_ptr.is_null() && data_len > 0 {
        debug!("FFI Warning: Null data_ptr for non-zero data_len in event {}", event_type);
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
            debug!("FFI Warning: No callback registered in engine for event {}", event_type);
        }
    } else {
        debug!("FFI Warning: Engine not initialized for event {}", event_type);
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
        error!("FFI Error: libc::malloc failed for event {}", event_type);
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
/// Returns a raw pointer to a C string. Caller MUST call `introvert_free_string()` on the returned pointer.
/// Returns null on failure (entropy generation or string conversion error).
#[no_mangle]
pub extern "C" fn introvert_generate_mnemonic() -> *mut c_char {
    let mut entropy = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut entropy);
    let mnemonic = match Mnemonic::from_entropy_in(Language::English, &entropy) {
        Ok(m) => m,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(mnemonic.to_string()) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
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

    #[cfg(debug_assertions)]
    {
        // In debug mode, verify the pointer is readable before dereferencing
        unsafe {
            let test_slice = std::slice::from_raw_parts(seed_ptr, 32);
            debug_assert!(!test_slice.iter().all(|&b| b == 0), "Seed pointer points to all-zero memory");
        }
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

    #[cfg(debug_assertions)]
    {
        unsafe {
            let test_slice = std::slice::from_raw_parts(seed_ptr, 32);
            debug_assert!(!test_slice.iter().all(|&b| b == 0), "Seed pointer points to all-zero memory");
        }
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
        .map(|p| p.join("drive").to_string_lossy().into_owned())
        .unwrap_or_else(|| "/tmp/drive".to_string());

    let is_anchor_mode = storage.is_anchor_mode_enabled();
    let is_tunnel_mode = storage.is_tunnel_mode_enabled();

    let daily_engine = Arc::new(crate::economy::daily_rewards::DailyRewardEngine::new(storage.clone()));
    if let Ok(Some((_, _, _, _, tier))) = storage.get_profile() {
        daily_engine.set_prestige_tier(tier as u8);
    }

    *engine_lock = Some(Engine {
        runtime,
        identity,
        storage: storage.clone(),
        reward_tracker,
        solana_client,
        daily_reward_engine: Some(daily_engine),
        session_encryption_key,
        network_tx: RwLock::new(None),
        network_callback: RwLock::new(None),
        is_anchor_mode: RwLock::new(is_anchor_mode),
        is_tunnel_mode: RwLock::new(is_tunnel_mode),
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

    // If network is already started, DO NOT start a duplicate service!
    if engine.network_tx.read().is_some() {
        dispatch_debug_log("Network already started. Ignoring duplicate start call.");
        return FfiResult::success();
    }

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
        match NetworkService::new(NetworkConfig {
            keypair,
            command_rx: rx,
            command_tx: tx_clone,
            storage,
            reward_tracker,
            solana_client,
            local_static_secret,
            session_encryption_key,
            enable_mdns: true,
            enable_listeners: true,
            tcp_port,
            enable_relay_server,
            max_connections,
            liveness_interval_secs,
            downloads_dir,
            is_stress_test: false,
        }).await {
            Ok(service) => {
                dispatch_debug_log("NetworkService initialized. Running swarm...");
                service.run().await;
            }
            Err(e) => {
                let err_msg = format!("Failed to start network service: {}", e);
                error!("{}", err_msg);
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
    let daily_engine = engine.daily_reward_engine.clone();

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

            // Daily reward cycle transition check
            if let Some(ref daily) = daily_engine {
                let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                if daily.needs_cycle_transition(&today) {
                    daily.transition_cycle(&today, &tracker);
                    let bal = solana.fetch_balance(&my_pubkey).await.unwrap_or(0);
                    daily.set_snapshot_balance(bal);
                }
                daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
                    activity_type: crate::economy::daily_rewards::ActivityType::UptimeSeconds,
                    peer_id: None,
                    value: 30,
                    is_foreground: true,
                    message_len: None,
                    is_self: false,
                    is_rbn: false,
                    proof_hash: None,
                });
            }

            let balance = solana.fetch_balance(&my_pubkey).await.unwrap_or(0);
            let sol_balance = solana.fetch_sol_balance(&my_pubkey).await.unwrap_or(0);
            let usdc_balance = solana.fetch_token_balance(&my_pubkey, &usdc_mint).await.unwrap_or(0);
            let pending = tracker.get_pending_rewards();
            let total_relayed = tracker.get_total_relayed();

            let mut stats = json!({
                "intr_balance": balance,
                "sol_balance": sol_balance,
                "usdc_balance": usdc_balance,
                "pending_rewards": pending,
                "total_relayed": total_relayed,
                "sol_address": my_pubkey.to_string(), 
                "treasury_address": treasury_pubkey.to_string(),
                "token_name": "INTR"
            });

            // Add real-time daily earnings from DailyRewardEngine
            if let Some(ref daily) = daily_engine {
                let earnings = daily.get_realtime_earnings();
                stats["daily_earnings"] = earnings;
            }

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

    // If it's a group share, we might not have a specific target peer yet.
    // We use the local PeerId as a placeholder if peer_id_str is empty.
    let peer_id = if peer_id_str.is_empty() && group_id.is_some() {
        engine.identity.peer_id
    } else {
        match PeerId::from_str(&peer_id_str) {
            Ok(pid) => pid,
            Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
        }
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::SendFile { peer_id, file_path, group_id, transfer_id: None }).await;
    });

    FfiResult::success()
}

/// Cancels an active file transfer by transfer_id.
#[no_mangle]
pub extern "C" fn introvert_network_cancel_file_transfer(
    transfer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if transfer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let transfer_id = unsafe { CStr::from_ptr(transfer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::CancelFileTransfer { transfer_id }).await;
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

/// Explicitly triggers a network diagnostics recheck / redial sequence for a peer.
#[no_mangle]
pub extern "C" fn introvert_network_recheck_connection(peer_id_ptr: *const c_char) -> FfiResult {
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
        let _ = tx.send(NetworkCommand::RecheckConnection { peer_id }).await;
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
    reply_to_ptr: *const c_char,
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
    let reply_to = if reply_to_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(reply_to_ptr).to_string_lossy().into_owned() }) };

    let peer_id = match PeerId::from_str(&peer_id_str) {
        Ok(pid) => pid,
        Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
    };

    // Privacy gate: check if peer is a contact (verified via dual handshake)
    match engine.storage.get_contact(&peer_id_str) {
        Ok(Some(_)) => {},
        _ => return FfiResult::error(-14, "Privacy Restriction: recipient is not in your contacts list. Handshake required."),
    };

    // Record daily reward activity
    if let Some(ref daily) = engine.daily_reward_engine {
        daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
            activity_type: crate::economy::daily_rewards::ActivityType::MessageSent,
            peer_id: Some(peer_id_str.clone()),
            value: 1,
            is_foreground: true,
            message_len: Some(message.len()),
            is_self: false,
            is_rbn: false,
            proof_hash: None,
        });
    }

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let msg_id = format!("m_{}_{}", peer_id_str, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let mid_cb = msg_id.clone();
        match tx.send(NetworkCommand::SendSignaling { peer_id, msg_id, message, reply_to }).await {
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
    media_type: u8,
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
        match tx.send(NetworkCommand::InitiateWebRtc { peer_id, media_type }).await {
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

/// Forwards a raw flutter_webrtc SDP/ICE signal JSON to a remote peer via the Rust mesh.
/// This is used when flutter_webrtc handles the WebRTC media stack natively on the Flutter side.
/// The signal is forwarded as SignalingPayload::WebRtcNative over the encrypted libp2p mesh.
#[no_mangle]
pub extern "C" fn introvert_webrtc_send_native_signal(
    peer_id_ptr: *const c_char,
    json_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || json_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let json = unsafe { CStr::from_ptr(json_ptr).to_string_lossy().into_owned() };

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
        let _ = tx.send(NetworkCommand::ForwardWebRtcNative { peer_id, json }).await;
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
        Ok(Some((name, handle, avatar, privacy_mode, prestige_tier))) => {
            let json = json!({ "name": name, "handle": handle, "avatar": avatar, "privacy_mode": privacy_mode, "prestige_tier": prestige_tier }).to_string();
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
    handle_ptr: *const c_char,
    avatar_ptr: *const c_char,
    privacy_mode: i32,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let name = if name_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(name_ptr).to_str().unwrap_or_default() }) };
    let handle = if handle_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(handle_ptr).to_str().unwrap_or_default() }) };
    let avatar = if avatar_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(avatar_ptr).to_str().unwrap_or_default() }) };

    match engine.storage.set_profile(name, handle, avatar, privacy_mode) {
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
                    "retention_hours": c.retention_seconds,
                    "handle": c.handle,
                    "prestige_tier": c.prestige_tier.unwrap_or(0),
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

/// Updates the local profile's prestige tier (called from Dart when INTR balance changes).
#[no_mangle]
pub extern "C" fn introvert_storage_set_profile_tier(tier: i32) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if tier < 0 || tier > 6 {
        return FfiResult::error(-2, "Tier must be 0-6");
    }

    match engine.storage.set_profile_tier(tier as u8) {
        Ok(_) => {
            if let Some(ref daily) = engine.daily_reward_engine {
                daily.set_prestige_tier(tier as u8);
            }
            FfiResult::success()
        }
        Err(e) => FfiResult::error(-1, &format!("Set tier error: {}", e)),
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

/// Accepts a pending WebRTC call from a peer with specified media tracks.
/// media_type: 0 = Audio, 1 = Video, 2 = Both.
#[no_mangle]
pub extern "C" fn introvert_network_accept_call(
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
        let _ = tx.send(NetworkCommand::AcceptWebRtc { peer_id, media_type }).await;
    });

    FfiResult::success()
}

/// Rejects a pending WebRTC call from a peer.
#[no_mangle]
pub extern "C" fn introvert_network_reject_call(
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
        let _ = tx.send(NetworkCommand::RejectWebRtc { peer_id }).await;
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
            debug!("FFI: get_messages for peer {} returned {} rows", peer_id, messages.len());
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(content, timestamp, is_me, status, msg_id, reply_to)| {
                // Convert SQLite timestamp (YYYY-MM-DD HH:MM:SS) to ISO 8601 (YYYY-MM-DDTHH:MM:SSZ)
                let iso_timestamp = timestamp.replace(" ", "T") + "Z";
                serde_json::json!({
                    "content": content,
                    "timestamp": iso_timestamp,
                    "is_me": is_me,
                    "status": status,
                    "msg_id": msg_id,
                    "reply_to": reply_to
                })
            }).collect();

            let json = serde_json::to_string(&json_messages).unwrap_or_else(|_| "[]".to_string());
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Paginated version of get_messages — returns the most recent `limit` messages starting from `offset`.
/// offset=0, limit=50 returns the last 50 messages. offset=50 returns the next 50, etc.
#[no_mangle]
pub extern "C" fn introvert_storage_get_messages_paginated(
    peer_id_ptr: *const c_char,
    offset: u32,
    limit: u32,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let effective_limit = if limit == 0 { 50 } else { limit.min(500) };

    match engine.storage.get_messages_for_peer_paginated(&peer_id, offset, effective_limit) {
        Ok(messages) => {
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(content, timestamp, is_me, status, msg_id, reply_to)| {
                let iso_timestamp = timestamp.replace(" ", "T") + "Z";
                serde_json::json!({
                    "content": content,
                    "timestamp": iso_timestamp,
                    "is_me": is_me,
                    "status": status,
                    "msg_id": msg_id,
                    "reply_to": reply_to
                })
            }).collect();

            let json = serde_json::to_string(&json_messages).unwrap_or_else(|_| "[]".to_string());
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_storage_get_last_message(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.get_last_message_for_peer(&peer_id) {
        Ok(Some((content, timestamp, is_me, msg_id))) => {
            let iso_timestamp = timestamp.replace(" ", "T") + "Z";
            let json = serde_json::json!({
                "content": content,
                "timestamp": iso_timestamp,
                "is_me": is_me,
                "msg_id": msg_id
            });
            let json_str = serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string());
            FfiResult::binary(json_str.into_bytes())
        }
        Ok(None) => FfiResult::binary(b"null".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_storage_get_last_group_message(group_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.get_last_message_for_group(&group_id) {
        Ok(Some((sender_id, content, timestamp, msg_id))) => {
            let iso_timestamp = timestamp.replace(" ", "T") + "Z";
            let json = serde_json::json!({
                "sender_id": sender_id,
                "content": content,
                "timestamp": iso_timestamp,
                "msg_id": msg_id
            });
            let json_str = serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string());
            FfiResult::binary(json_str.into_bytes())
        }
        Ok(None) => FfiResult::binary(b"null".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Batch: returns last message for ALL contacts in one call (replaces N individual calls).
#[no_mangle]
pub extern "C" fn introvert_storage_get_last_messages_all() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_last_messages_all() {
        Ok(json) => {
            let json_str = serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string());
            FfiResult::binary(json_str.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Batch: returns last message for ALL groups in one call.
#[no_mangle]
pub extern "C" fn introvert_storage_get_last_group_messages_all() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_last_group_messages_all() {
        Ok(json) => {
            let json_str = serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string());
            FfiResult::binary(json_str.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

fn build_local_sovereign_identity(engine: &Engine) -> anyhow::Result<crate::identity::SovereignIdentity> {
    let identity = &engine.identity;
    let storage = &engine.storage;

    let local_static_secret = NodeIdentity::derive_e2ee_key(identity.seed)?;
    let local_static_public = x25519_dalek::PublicKey::from(&local_static_secret);

    let solana_signing_key = NodeIdentity::derive_solana_keypair(identity.seed)?;
    let solana_address = solana_sdk::pubkey::Pubkey::new_from_array(solana_signing_key.verifying_key().to_bytes()).to_string();

    let (local_name, local_handle, local_avatar, _, local_tier) = storage.get_profile().unwrap_or(None).unwrap_or((None, None, None, 0, 0));

    Ok(crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true,
        retention_seconds: 0,
        handle: local_handle,
        prestige_tier: Some(local_tier as u8),
    })
}

/// Initiates a Magic Wormhole invite session using the global network callback.
#[no_mangle]
pub extern "C" fn introvert_wormhole_start() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let _identity = Arc::clone(&engine.identity);
    let storage = Arc::clone(&engine.storage);

    let my_identity = match build_local_sovereign_identity(engine) {
        Ok(id) => id,
        Err(_) => return FfiResult::error(-14, "Identity derivation failed"),
    };

    let handle = engine.runtime.spawn(async move {
        dispatch_debug_log("Wormhole: Starting invite creation process...");

        let invite_result = tokio::time::timeout(
            Duration::from_secs(60),
            crate::network::wormhole::create_invite(my_identity)
        ).await;

        match invite_result {
            Ok(Ok((code, handshake_future))) => {
                dispatch_debug_log(&format!("Wormhole: Code generated successfully: {}", code));
                // Add a small delay for UI stability
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // Emit the code to the UI (Event Type 6)
                dispatch_global_event(6, code.as_bytes());
                
                // Wait for the peer to connect and exchange identity with a 90-second timeout
                let handshake_res = tokio::time::timeout(
                    Duration::from_secs(90),
                    handshake_future
                ).await;

                match handshake_res {
                    Ok(Ok(peer_identity)) => {
                        dispatch_debug_log("Wormhole: Handshake SUCCESS. Persisting contact...");
                        let _ = storage.upsert_sovereign_contact(&peer_identity, true, false);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        // Add a small delay to ensure DB is flushed before UI reloads
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        dispatch_global_event(7, peer_identity.peer_id.as_bytes());
                    }
                    Ok(Err(e)) => {
                        let err_msg = format!("ERROR:HANDSHAKE_FAILED:{}", e);
                        error!("Wormhole handshake failed: {}", e);
                        dispatch_global_event(6, err_msg.as_bytes());
                    }
                    Err(_) => {
                        error!("Wormhole handshake timed out");
                        dispatch_global_event(6, "ERROR:TIMEOUT:Handshake timed out. Peer might have disconnected.".as_bytes());
                    }
                }
            }
            Ok(Err(e)) => {
                let err_msg = format!("ERROR:CREATE_FAILED:{}", e);
                error!("Failed to create Wormhole invite: {}", e);
                dispatch_global_event(6, err_msg.as_bytes());
            }
            Err(_) => {
                error!("Wormhole invite creation timed out");
                dispatch_global_event(6, "ERROR:TIMEOUT:Mailbox relay unreachable. Please check your connection or firewall (Port 443/WSS).".as_bytes());
            }
        }
    });

    {
        let mut task_lock = WORMHOLE_TASK.lock();
        if let Some(h) = task_lock.replace(handle) {
            h.abort();
            debug!("Wormhole: Aborted previous active session/task.");
        }
    }

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

    let _identity = Arc::clone(&engine.identity);
    let storage = Arc::clone(&engine.storage);
    
    let my_identity = match build_local_sovereign_identity(engine) {
        Ok(id) => id,
        Err(_) => return FfiResult::error(-14, "Identity derivation failed"),
    };

    let handle = engine.runtime.spawn(async move {
        dispatch_debug_log("Wormhole: Starting join process...");
        let accept_res = tokio::time::timeout(
            Duration::from_secs(60),
            crate::network::wormhole::accept_invite(code, my_identity)
        ).await;

        match accept_res {
            Ok(Ok(handshake_future)) => {
                dispatch_debug_log("Wormhole: Linked to peer. Waiting for handshake...");
                let handshake_res = tokio::time::timeout(
                    Duration::from_secs(90),
                    handshake_future
                ).await;

                match handshake_res {
                    Ok(Ok(peer_identity)) => {
                        dispatch_debug_log("Wormhole: Join SUCCESS. Persisting contact...");
                        let _ = storage.upsert_sovereign_contact(&peer_identity, true, false);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        // Add a small delay to ensure DB is flushed before UI reloads
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        dispatch_global_event(7, peer_identity.peer_id.as_bytes());
                    }
                    Ok(Err(e)) => {
                        let err_msg = format!("ERROR:JOIN_HANDSHAKE_FAILED:{}", e);
                        error!("Wormhole join handshake failed: {}", e);
                        dispatch_global_event(6, err_msg.as_bytes());
                    }
                    Err(_) => {
                        error!("Wormhole join handshake timed out");
                        dispatch_global_event(6, "ERROR:TIMEOUT:Join handshake timed out".as_bytes());
                    }
                }
            }
            Ok(Err(e)) => {
                let err_msg = format!("ERROR:JOIN_FAILED:{}", e);
                error!("Failed to join Wormhole session: {}", e);
                dispatch_global_event(6, err_msg.as_bytes());
            }
            Err(_) => {
                error!("Wormhole join connection timed out");
                dispatch_global_event(6, "ERROR:TIMEOUT:Join connection timed out. Please check your connection or firewall (Port 443/WSS).".as_bytes());
            }
        }
    });

    {
        let mut task_lock = WORMHOLE_TASK.lock();
        if let Some(h) = task_lock.replace(handle) {
            h.abort();
            debug!("Wormhole: Aborted previous active session/task.");
        }
    }

    FfiResult::success()
}

/// Aborts any active Magic Wormhole invite or join session.
#[no_mangle]
pub extern "C" fn introvert_wormhole_abort() -> FfiResult {
    let mut task_lock = WORMHOLE_TASK.lock();
    if let Some(h) = task_lock.take() {
        h.abort();
        debug!("Wormhole: Aborted active session/task on user request.");
    }
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

    let storage = Arc::clone(&engine.storage);
    let _ = storage.set_anchor_mode_enabled(enabled);

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

/// Sets the node's WebSocket secure tunnel mode.
#[no_mangle]
pub extern "C" fn introvert_network_set_tunnel_mode(enabled: bool) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    {
        let mut tunnel_lock = engine.is_tunnel_mode.write();
        *tunnel_lock = enabled;
    }

    let storage = Arc::clone(&engine.storage);
    let _ = storage.set_tunnel_mode_enabled(enabled);

    FfiResult::success()
}

/// Returns 1 if Tunnel Mode is enabled, 0 otherwise.
#[no_mangle]
pub extern "C" fn introvert_network_get_tunnel_mode() -> i32 {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        if *engine.is_tunnel_mode.read() { 1 } else { 0 }
    } else {
        0
    }
}

/// Returns the number of currently active peer connections in the swarm.
#[no_mangle]
pub extern "C" fn introvert_network_get_active_peer_count() -> i32 {
    ACTIVE_PEER_COUNT.load(std::sync::atomic::Ordering::Relaxed) as i32
}

// --- Intro-Claw AI Engine Mode ---

/// Returns the Intro-Claw AI mode: 0 = 100% Offline (Deterministic Macros), 1 = Hybrid AI Assistant.
#[no_mangle]
pub extern "C" fn intro_claw_get_ai_mode() -> i32 {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        let storage = Arc::clone(&engine.storage);
        storage.get_intro_claw_ai_mode()
    } else {
        0 // Default: 100% Offline
    }
}

/// Sets the Intro-Claw AI mode and optionally the external LLM API key.
/// mode: 0 = 100% Offline, 1 = Hybrid AI Assistant.
/// api_key: The external LLM API key (stored encrypted via SQLCipher). Pass empty string to clear.
#[no_mangle]
pub extern "C" fn intro_claw_set_ai_mode(mode: i32, api_key: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    let api_key_str = if api_key.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(api_key).to_string_lossy().to_string() }
    };

    let storage = Arc::clone(&engine.storage);
    match storage.set_intro_claw_ai_mode(mode, &api_key_str) {
        Ok(()) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Failed to save AI mode: {}", e)),
    }
}

/// Returns the Intro-Claw API key (encrypted via SQLCipher master key).
#[no_mangle]
pub extern "C" fn intro_claw_get_api_key() -> *mut c_char {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        let storage = Arc::clone(&engine.storage);
        let key = storage.get_intro_claw_api_key();
        match CString::new(key) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// Manually trigger an Intro-Claw maintenance tick.
#[no_mangle]
pub extern "C" fn intro_claw_trigger_tick() -> FfiResult {
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
        let _ = tx.send(NetworkCommand::IntroClawTick {
            battery_pct: 100,
            is_background: false,
            connected_peers: Vec::new(),
            mdns_discovered: Vec::new(),
        }).await;
    });
    FfiResult::success()
}

/// Enable or disable the Intro-Claw automation engine.
#[no_mangle]
pub extern "C" fn intro_claw_set_active(active: bool) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    // Persist the active state
    let _ = engine.storage.set_intro_claw_active(active);
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx.send(NetworkCommand::IntroClawSetActive { active }).await;
        });
    }
    FfiResult::success()
}

/// Set Intro-Claw node mode (for anchor/always-on nodes)
/// Node mode enables aggressive optimizations: proactive file caching,
/// aggressive dead letter processing, bandwidth-aware serving.
#[no_mangle]
pub extern "C" fn intro_claw_set_node_mode(enabled: bool) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    // Persist node mode state
    let _ = engine.storage.set_intro_claw_node_mode(enabled);
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx.send(NetworkCommand::IntroClawSetNodeMode { enabled }).await;
        });
    }
    FfiResult::success()
}

/// Returns Intro-Claw status as JSON: { "is_active": bool, "log_count": int }
#[no_mangle]
pub extern "C" fn intro_claw_get_status() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    let is_active = engine.storage.get_intro_claw_active();
    let status = serde_json::json!({
        "is_active": is_active,
        "mode": "local",
    });
    let json_str = status.to_string();
    FfiResult::binary(json_str.into_bytes())
}

/// Process an assistant query — local only, sandboxed, no external calls
#[no_mangle]
pub extern "C" fn intro_claw_process_query(query_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    if query_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let raw_query = unsafe { CStr::from_ptr(query_ptr).to_string_lossy().into_owned() };

    let response = crate::intro_claw::process_assistant_query(
        &engine.storage, &raw_query,
    );

    match serde_json::to_string(&response) {
        Ok(json) => FfiResult::binary(json.into_bytes()),
        Err(e) => FfiResult::error(-5, &format!("JSON serialization failed: {}", e)),
    }
}

/// Run network recon and return markdown report
#[no_mangle]
pub extern "C" fn intro_claw_run_network_recon() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(runtime) = rt {
            let report = runtime.block_on(async move {
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(NetworkCommand::IntroClawNetworkRecon { result_tx }).await;
                result_rx.await.unwrap_or_else(|_| "Recon request failed".to_string())
            });
            return FfiResult::binary(report.into_bytes());
        }
    }
    FfiResult::error(-11, "Network command channel unavailable")
}

/// Heal connection to a specific peer
#[no_mangle]
pub extern "C" fn intro_claw_heal_peer(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id: libp2p::PeerId = match peer_id_str.parse() {
        Ok(p) => p,
        Err(e) => return FfiResult::error(-12, &format!("Invalid peer ID: {}", e)),
    };
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(runtime) = rt {
            let report = runtime.block_on(async move {
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(NetworkCommand::IntroClawNetworkHeal { peer_id, result_tx }).await;
                result_rx.await.unwrap_or_else(|_| "Heal request failed".to_string())
            });
            return FfiResult::binary(report.into_bytes());
        }
    }
    FfiResult::error(-11, "Network command channel unavailable")
}

/// Get Intro-Claw activity log as JSON array
#[no_mangle]
pub extern "C" fn intro_claw_get_activity_log() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    // Access via network command channel
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(runtime) = rt {
            let log_json = runtime.block_on(async move {
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(NetworkCommand::IntroClawGetActivityLog { result_tx }).await;
                result_rx.await.unwrap_or_else(|_| "[]".to_string())
            });
            return FfiResult::binary(log_json.into_bytes());
        }
    }
    FfiResult::error(-11, "Network command channel unavailable")
}

#[no_mangle]
pub extern "C" fn introvert_get_peer_id() -> *mut c_char {
    let lock = ENGINE.read();
    if let Some(engine) = lock.as_ref() {
        let peer_id = engine.identity.peer_id.to_string();
        match CString::new(peer_id) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
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

/// Notify Intro-Claw that a VoIP call has started
#[no_mangle]
pub extern "C" fn intro_claw_voip_start_call(peer_id_ptr: *const c_char, is_video: i32) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let is_video_bool = is_video != 0;
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx.send(NetworkCommand::IntroClawVoipStartCall { peer_id, is_video: is_video_bool }).await;
        });
    }
    FfiResult::success()
}

/// Notify Intro-Claw that a VoIP call has ended
#[no_mangle]
pub extern "C" fn intro_claw_voip_end_call() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx.send(NetworkCommand::IntroClawVoipEndCall).await;
        });
    }
    FfiResult::success()
}

/// Record a VoIP quality sample
#[no_mangle]
pub extern "C" fn intro_claw_voip_record_sample(
    rtt_ms: u64,
    packet_loss_pct: f64,
    jitter_ms: u64,
    bitrate_kbps: u64,
    is_relayed: i32,
    codec_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    if codec_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let codec = unsafe { CStr::from_ptr(codec_ptr).to_string_lossy().into_owned() };
    let is_relayed_bool = is_relayed != 0;
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx.send(NetworkCommand::IntroClawVoipRecordSample {
                rtt_ms, packet_loss_pct, jitter_ms, bitrate_kbps, is_relayed: is_relayed_bool, codec,
            }).await;
        });
    }
    FfiResult::success()
}

/// Get VoIP call quality summary as string
#[no_mangle]
pub extern "C" fn intro_claw_voip_get_quality() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(runtime) = rt {
            let quality = runtime.block_on(async move {
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(NetworkCommand::IntroClawVoipGetQuality { result_tx }).await;
                result_rx.await.unwrap_or_else(|_| "No active call".to_string())
            });
            return FfiResult::binary(quality.into_bytes());
        }
    }
    FfiResult::error(-11, "Network command channel unavailable")
}

/// Check if VoIP should downgrade quality
#[no_mangle]
pub extern "C" fn intro_claw_voip_should_downgrade() -> i32 {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return 0,
    };
    
    // Check if we should downgrade based on VoIP quality samples
    // This is a fast synchronous check — no network command needed
    // The caller should check this periodically during a call
    let tx_lock = engine.network_tx.read();
    if let Some(_tx) = tx_lock.as_ref() {
        // For now, return 0 — the actual downgrade logic will be triggered
        // by the IntroClaw tick when it detects poor quality
        0
    } else {
        0
    }
}

/// Get VoIP downgrade recommendation
/// Returns: "none", "audio_only", "low_bitrate"
#[no_mangle]
pub extern "C" fn intro_claw_voip_get_downgrade_recommendation() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };
    
    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let rt = tokio::runtime::Runtime::new().ok();
        if let Some(runtime) = rt {
            let recommendation = runtime.block_on(async move {
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(NetworkCommand::IntroClawVoipGetDowngradeRecommendation { result_tx }).await;
                result_rx.await.unwrap_or_else(|_| "none".to_string())
            });
            return FfiResult::binary(recommendation.into_bytes());
        }
    }
    FfiResult::error(-11, "Network command channel unavailable")
}

/// Reclaims leaked binary memory once Dart has finished copying it.
/// The `_len` parameter is intentionally unused — `libc::free` does not require the size.
/// It is kept in the signature for API consistency with the Dart side, which passes the length
/// for its own bookkeeping. The underscore prefix signals intentional non-use to the compiler.
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

/// Retrieves metadata for a specific file in the local encrypted Drive by its hash.
#[no_mangle]
pub extern "C" fn introvert_drive_get_by_hash(
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

    match engine.storage.get_drive_file_by_hash(&file_hash) {
        Ok(Some(file)) => {
            let json = serde_json::to_string(&file).unwrap_or_default();
            FfiResult::binary(json.into_bytes())
        }
        Ok(None) => FfiResult::binary(b"{}".to_vec()),
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
    let local_profile = engine.storage.get_profile().ok().flatten();
    let creator_alias = local_profile.as_ref().and_then(|(n, _, _, _, _)| n.clone());
    let creator_avatar = local_profile.and_then(|(_, _, a, _, _)| a);

    let creator_member = crate::network::GroupMemberMetadata {
        peer_id: creator_peer_id,
        pubkey: creator_pubkey,
        role: crate::network::GroupRole::Creator,
        alias: creator_alias,
        avatar_base64: creator_avatar,
    };

    let mut members = vec![creator_member];

    crate::dispatch_debug_log(&format!("introvert_group_create: Creating group {} with name '{}'", group_id, name));
    let initial_peer_ids: Vec<String> = serde_json::from_str(&members_json_str).unwrap_or_default();
    crate::dispatch_debug_log(&format!("introvert_group_create: Initial peer IDs count: {}", initial_peer_ids.len()));
    for peer_id_str in initial_peer_ids {
        crate::dispatch_debug_log(&format!("introvert_group_create: Looking up contact for peer: {}", peer_id_str));
        match engine.storage.get_contact(&peer_id_str) {
            Ok(Some(contact)) => {
                crate::dispatch_debug_log(&format!("introvert_group_create: Found contact for {}. static_key prefix: {}", peer_id_str, hex::encode(&contact.static_key[0..4.min(contact.static_key.len())])));
                members.push(crate::network::GroupMemberMetadata {
                    peer_id: peer_id_str,
                    pubkey: contact.p2p_pubkey,
                    role: crate::network::GroupRole::Member,
                    alias: contact.local_alias.or(contact.global_name),
                    avatar_base64: contact.avatar_base64,
                });
            }
            Ok(None) => {
                crate::dispatch_debug_log(&format!("introvert_group_create: ⚠️ Contact {} NOT found in storage!", peer_id_str));
            }
            Err(e) => {
                crate::dispatch_debug_log(&format!("introvert_group_create: ❌ Error loading contact {}: {:?}", peer_id_str, e));
            }
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
            // Subscribe to gossipsub topic for the newly created group so the creator receives mesh traffic
            let _ = tx.send(crate::network::NetworkCommand::SubscribeGossipsub { group_id: group_id_clone.clone() }).await;

            for m in members_clone {
                if m.peer_id == my_peer_id { continue; }
                if let Ok(pid) = PeerId::from_str(&m.peer_id) {
                    crate::dispatch_debug_log(&format!("introvert_group_create: Sending invite to member {}", m.peer_id));
                    match storage.get_contact(&m.peer_id) {
                        Ok(Some(contact)) => {
                            match crate::network::group::GroupManager::wrap_group_secret(&secret, &contact.static_key) {
                                Ok(wrapped) => {
                                    crate::dispatch_debug_log(&format!("introvert_group_create: Successfully wrapped secret for {}", m.peer_id));
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
                                Err(e) => {
                                    crate::dispatch_debug_log(&format!("introvert_group_create: ❌ Failed to wrap group secret for {}: {:?}", m.peer_id, e));
                                }
                            }
                        }
                        _ => {
                            crate::dispatch_debug_log(&format!("introvert_group_create: ❌ Failed to load contact from invite loop for {}", m.peer_id));
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
    reply_to_ptr: *const c_char,
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
    let reply_to = if reply_to_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(reply_to_ptr).to_string_lossy().into_owned() }) };

    let group_secret_vec = match engine.storage.load_group_secret(&group_id) {
        Ok(Some(s)) => {
            crate::dispatch_debug_log(&format!("introvert_group_send_message: Loaded group secret. Hex prefix: {}", hex::encode(&s[0..4.min(s.len())])));
            s
        }
        _ => return FfiResult::error(-1, "Group secret not found"),
    };
    if group_secret_vec.len() != 32 {
        return FfiResult::error(-2, "Invalid group secret length");
    }
    let mut group_secret = [0u8; 32];
    group_secret.copy_from_slice(&group_secret_vec);
    if group_secret.iter().all(|&b| b == 0) {
        return FfiResult::error(-6, "Group secret is all-zeros");
    }

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

    let mut msg_id = format!("gm_{}_{}_{:08x}", group_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0), rand::random::<u32>());
    if message.starts_with("[FILE]:") {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&message[7..]) {
            if let Some(tid) = meta.get("transfer_id").and_then(|v| v.as_str()) {
                msg_id = tid.to_string();
            }
        }
    }
    let action = crate::network::GroupAction::Message { content_encrypted, msg_id: msg_id.clone(), reply_to: reply_to.clone() };
    let signed = match crate::network::group::GroupManager::sign_action(group_id.clone(), action, &engine.identity.keypair) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
    };

    let my_peer_id = engine.identity.peer_id.to_string();
    if let Err(e) = engine.storage.store_group_message(&group_id, &my_peer_id, &msg_id, &message, true, reply_to.as_deref()) {
        return FfiResult::error(-5, &format!("Database error: {}", e));
    }

    // Record daily reward activity for group message
    if let Some(ref daily) = engine.daily_reward_engine {
        daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
            activity_type: crate::economy::daily_rewards::ActivityType::GroupMessageSent,
            peer_id: Some(group_id.clone()),
            value: 1,
            is_foreground: true,
            message_len: Some(message.len()),
            is_self: false,
            is_rbn: false,
            proof_hash: None,
        });
    }

    let tx_lock = engine.network_tx.read();
    if let Some(tx) = tx_lock.as_ref() {
        let tx = tx.clone();
        let payload = crate::network::SignalingPayload::GroupAction(signed);
        let group_id_clone = group_id.clone();
        let my_peer_id_clone = my_peer_id.clone();
        let storage = engine.storage.clone();

        engine.runtime.spawn(async move {
            crate::dispatch_debug_log(&format!("introvert_group_send_message: Spawning send loop for group {}", group_id_clone));
            match storage.get_group_members(&group_id_clone) {
                Ok(Some(members_json)) => {
                    let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                    crate::dispatch_debug_log(&format!("introvert_group_send_message: Group {} has {} members", group_id_clone, members.len()));
                    for m in members {
                        if m.peer_id == my_peer_id_clone { continue; }
                        match PeerId::from_str(&m.peer_id) {
                            Ok(pid) => {
                                crate::dispatch_debug_log(&format!("introvert_group_send_message: Forwarding GroupAction to member {}", m.peer_id));
                                let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                            }
                            Err(e) => {
                                crate::dispatch_debug_log(&format!("introvert_group_send_message: ❌ Failed to parse peer ID '{}': {:?}", m.peer_id, e));
                            }
                        }
                    }
                }
                Ok(None) => {
                    crate::dispatch_debug_log(&format!("introvert_group_send_message: ⚠️ Group {} not found or has no members", group_id_clone));
                }
                Err(e) => {
                    crate::dispatch_debug_log(&format!("introvert_group_send_message: ❌ Error loading group members for {}: {:?}", group_id_clone, e));
                }
            }
        });
    } else {
        crate::dispatch_debug_log("introvert_group_send_message: ❌ network_tx is None!");
    }

    FfiResult::success()
}

/// Retrieves all unread message counts.
#[no_mangle]
pub extern "C" fn introvert_storage_get_unread_counts() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_unread_counts() {
        Ok(counts) => {
            let json = serde_json::to_string(&counts).unwrap_or_default();
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Updates the status of all messages in a group.
#[no_mangle]
pub extern "C" fn introvert_storage_update_group_message_status(
    group_id_ptr: *const c_char,
    status: u8,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.update_group_message_status(&group_id, status) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

/// Updates the status of all messages for a peer.
#[no_mangle]
pub extern "C" fn introvert_storage_update_message_status_for_peer(
    peer_id_ptr: *const c_char,
    status: u8,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.update_message_status_for_peer(&peer_id, status) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
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
            debug!("[FFI] Found {} groups in database", groups.len());
            let mut groups_json = Vec::new();
            for (gid, name, members, desc, retention) in groups {
                groups_json.push(vec![
                    serde_json::Value::String(gid),
                    serde_json::Value::String(name),
                    serde_json::Value::String(members),
                    serde_json::Value::String(desc),
                    serde_json::Value::Number(retention.into()),
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

    let members_json = engine.storage.get_group_members(&group_id).ok().flatten();
    let group_members: Vec<crate::network::GroupMemberMetadata> = if let Some(ref mj) = members_json {
        serde_json::from_str(mj).unwrap_or_default()
    } else {
        Vec::new()
    };

    match engine.storage.get_group_messages(&group_id) {
        Ok(msgs) => {
            // Pre-fetch all contacts to avoid N+1 queries
            let contacts_map: std::collections::HashMap<String, crate::identity::SovereignIdentity> = 
                engine.storage.get_all_contacts().ok()
                    .map(|c| c.into_iter().map(|ci| (ci.peer_id.clone(), ci)).collect())
                    .unwrap_or_default();
            
            let mut msgs_json = Vec::new();
            for (sender_id, msg_id, content, timestamp, reply_to) in msgs {
                let (sender_name, sender_avatar) = if sender_id == my_peer_id {
                    // Local user name & avatar resolution
                    let profile = engine.storage.get_profile().ok().flatten();
                    let name = profile.as_ref().and_then(|(n, _, _, _, _)| n.clone().filter(|n| !n.is_empty()))
                        .unwrap_or_else(|| "me".to_string());
                    let avatar = profile.and_then(|(_, _, a, _, _)| a);
                    (name, avatar)
                } else {
                    // Resolution Priority:
                    // 1. Local Contact (Alias/GlobalName + Avatar)
                    // 2. Group Member Metadata (Alias + Avatar)
                    // 3. Raw Peer ID (truncated)

                    let contact_opt = contacts_map.get(&sender_id);

                    let mut name = contact_opt
                        .and_then(|c| c.local_alias.clone().or(c.global_name.clone()))
                        .unwrap_or_else(|| {
                            group_members.iter()
                                .find(|m| m.peer_id == sender_id)
                                .and_then(|m| m.alias.clone())
                                .unwrap_or_else(|| sender_id.clone())
                        });

                    let avatar = contact_opt
                        .and_then(|c| c.avatar_base64.clone())
                        .or_else(|| {
                            group_members.iter()
                                .find(|m| m.peer_id == sender_id)
                                .and_then(|m| m.avatar_base64.clone())
                        });

                    // If it's a raw PeerID (long string), truncate it for the UI
                    if name.len() > 30 && name == sender_id {
                        name = format!("Peer: {}...{}", &name[0..6], &name[name.len()-4..]);
                    }
                    (name, avatar)
                };

                let iso_timestamp = timestamp.replace(" ", "T") + "Z";
                msgs_json.push(vec![
                    serde_json::Value::String(sender_id),
                    serde_json::Value::String(sender_name),
                    serde_json::Value::String(content),
                    serde_json::Value::String(iso_timestamp),
                    serde_json::Value::String(msg_id),
                    reply_to.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                    sender_avatar.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
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

/// Admin approves a group join request.
#[no_mangle]
pub extern "C" fn introvert_group_approve_join(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
    alias_ptr: *const c_char,
    avatar_ptr: *const c_char,
    handle_ptr: *const c_char,
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
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let alias = if alias_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(alias_ptr).to_string_lossy().into_owned() }) };
    let avatar = if avatar_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(avatar_ptr).to_string_lossy().into_owned() }) };
    let handle = if handle_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(handle_ptr).to_string_lossy().into_owned() }) };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::ApproveGroupJoin {
            group_id,
            requester_peer_id: peer_id,
            alias,
            avatar,
            handle,
        }).await;
    });

    FfiResult::success()
}

/// Admin rejects a group join request.
#[no_mangle]
pub extern "C" fn introvert_group_reject_join(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
    reason_ptr: *const c_char,
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
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let reason = if reason_ptr.is_null() { "Access denied".to_string() } else { unsafe { CStr::from_ptr(reason_ptr).to_string_lossy().into_owned() } };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::RejectGroupJoin {
            group_id,
            requester_peer_id: peer_id,
            reason,
        }).await;
    });

    FfiResult::success()
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
    let my_peer_id = engine.identity.peer_id.to_string();
    let is_self = peer_id_str == my_peer_id;

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let mut members_json = None;
    if is_self {
        // If we are leaving, perform local deletion synchronously to ensure UI reflects it immediately.
        if let Ok(Some(group_info)) = engine.storage.get_group(&group_id) {
            members_json = Some(group_info.members_json.clone());
            let _ = engine.storage.delete_group(&group_id);
            crate::dispatch_global_event(22, group_id.as_bytes());
        }
    }

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::RemoveGroupMember { 
            group_id, 
            peer_id: peer_id_str,
            members_json,
        }).await;
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
    debug!("[FFI] Attempting to delete group: {}", group_id);

    if let Ok(Some(group_info)) = engine.storage.get_group(&group_id) {
        let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
        let is_creator = members.iter().any(|m| m.peer_id == my_peer_id && m.role == crate::network::GroupRole::Creator);
        debug!("[FFI] Group found. Is creator: {}", is_creator);
        
        // Only signal the mesh if we are the creator. 
        if is_creator {
            debug!("[FFI] Signaling mesh about group deletion");
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
    } else {
        debug!("[FFI] Group {} not found in storage during delete attempt", group_id);
    }

    match engine.storage.delete_group(&group_id) {
        Ok(_) => {
            debug!("[FFI] Successfully deleted group {} from local storage", group_id);
            crate::dispatch_global_event(22, group_id.as_bytes());
            FfiResult::success()
        },
        Err(e) => {
            error!("[FFI] FAILED to delete group {}: {}", group_id, e);
            FfiResult::error(-1, &format!("Database error: {}", e))
        },
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

/// Mutes a member in a group (Admin only).
#[no_mangle]
pub extern "C" fn introvert_group_mute_member(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let action = crate::network::GroupAction::MuteMember { peer_id };
    let signed = match crate::network::group::GroupManager::sign_action(group_id.clone(), action, &engine.identity.keypair) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
    };
    let payload = crate::network::SignalingPayload::GroupAction(signed);
    let storage = engine.storage.clone();
    let my_peer_id = engine.identity.peer_id.to_string();
    engine.runtime.spawn(async move {
        if let Ok(Some(members_json)) = storage.get_group_members(&group_id) {
            let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
            for m in members {
                if m.peer_id == my_peer_id { continue; }
                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                    let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                }
            }
        }
    });

    FfiResult::success()
}

/// Unmutes a member in a group (Admin only).
#[no_mangle]
pub extern "C" fn introvert_group_unmute_member(
    group_id_ptr: *const c_char,
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() || peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let action = crate::network::GroupAction::UnmuteMember { peer_id };
    let signed = match crate::network::group::GroupManager::sign_action(group_id.clone(), action, &engine.identity.keypair) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
    };
    let payload = crate::network::SignalingPayload::GroupAction(signed);
    let storage = engine.storage.clone();
    let my_peer_id = engine.identity.peer_id.to_string();
    engine.runtime.spawn(async move {
        if let Ok(Some(members_json)) = storage.get_group_members(&group_id) {
            let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
            for m in members {
                if m.peer_id == my_peer_id { continue; }
                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                    let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                }
            }
        }
    });

    FfiResult::success()
}

/// Retrieves the list of muted members in a group.
#[no_mangle]
pub extern "C" fn introvert_group_get_muted_members(
    group_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if group_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.get_group_muted_members(&group_id) {
        Ok(muted) => {
            let json = serde_json::to_string(&muted).unwrap_or_else(|_| "[]".to_string());
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Requests a fresh calculation of swarm statistics.
/// Results are dispatched via Global Event Type 30.
#[no_mangle]
pub extern "C" fn introvert_network_request_swarm_stats() -> FfiResult {
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
        let _ = tx.send(crate::network::NetworkCommand::RequestSwarmStats).await;
    });

    FfiResult::success()
}

/// Computes the SHA-256 hash of a local file.
#[no_mangle]
pub extern "C" fn introvert_network_compute_file_hash(
    file_path_ptr: *const c_char,
) -> FfiResult {
    if file_path_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }
    let file_path = unsafe { CStr::from_ptr(file_path_ptr).to_string_lossy().into_owned() };
    
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return FfiResult::error(-1, "File not found");
    }

    use sha2::{Sha256, Digest};
    use std::io::BufReader;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return FfiResult::error(-2, &format!("Failed to open file: {:?}", e)),
    };
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    if let Err(e) = std::io::copy(&mut reader, &mut hasher) {
        return FfiResult::error(-3, &format!("Failed to read file: {:?}", e));
    }
    let file_hash = format!("{:x}", hasher.finalize());

    FfiResult::binary(file_hash.into_bytes())
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

    let chunk_size = if group_id.is_some() { 64 * 1024 } else { 256 * 1024 };
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

    let peer = if peer_id_str.is_empty() {
        engine.identity.peer_id
    } else {
        match PeerId::from_str(&peer_id_str) {
            Ok(pid) => pid,
            Err(_) => return FfiResult::error(-12, "Invalid PeerId"),
        }
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
        total_size: total_size as u64,
        is_relayed,
        sender_peer_id: Some(peer_id_str.clone()),
        group_id,
    };

    // Record daily reward activity for file transfer
    if let Some(ref daily) = engine.daily_reward_engine {
        daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
            activity_type: crate::economy::daily_rewards::ActivityType::FileTransferSent,
            peer_id: Some(peer_id_str),
            value: 1,
            is_foreground: true,
            message_len: None,
            is_self: false,
            is_rbn: false,
            proof_hash: None,
        });
    }

    engine.runtime.spawn(async move {
        // Forward signaling directly to ourselves as if received from 'peer'
        let _ = tx.send(crate::network::NetworkCommand::HandleIncomingPayload { peer_id: peer, payload }).await;
    });

    FfiResult::success()
}

/// Resolves a persistent handle (i@handle) to a PeerId via DHT.
/// Result is dispatched via Global Event Type 33.
#[no_mangle]
pub extern "C" fn introvert_network_resolve_handle(
    handle_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if handle_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let mut handle = unsafe { CStr::from_ptr(handle_ptr).to_string_lossy().into_owned() };
    if !handle.starts_with("i@") {
        handle = format!("i@{}", handle);
    }

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::ResolveHandle { handle }).await;
    });

    FfiResult::success()
}

/// Initiates a handle claim process. Performs local PoW then broadcasts to RBNs.
/// Result is dispatched via Global Event Type 34.
#[no_mangle]
pub extern "C" fn introvert_network_claim_handle(
    handle_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if handle_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let mut handle = unsafe { CStr::from_ptr(handle_ptr).to_string_lossy().into_owned() };
    if !handle.starts_with("i@") {
        handle = format!("i@{}", handle);
    }

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::ClaimHandle { handle }).await;
    });

    FfiResult::success()
}

/// Queries the local registry for a handle's verified status.
#[no_mangle]
pub extern "C" fn introvert_storage_get_handle_status(
    handle_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if handle_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let mut handle = unsafe { CStr::from_ptr(handle_ptr).to_string_lossy().into_owned() };
    if !handle.starts_with("i@") {
        handle = format!("i@{}", handle);
    }

    match engine.storage.get_handle_claim(&handle) {
        Ok(Some((peer_id, timestamp, signatures, verified))) => {
            let json = json!({
                "peer_id": peer_id,
                "timestamp": timestamp,
                "signatures": signatures,
                "verified": verified
            }).to_string();
            FfiResult::binary(json.into_bytes())
        }
        Ok(None) => FfiResult::binary(b"{}".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Returns the local user's verified handle (immutable once set). Returns empty string if none.
#[no_mangle]
pub extern "C" fn introvert_storage_get_local_handle() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    match engine.storage.get_local_handle() {
        Ok(Some(handle)) => FfiResult::binary(handle.into_bytes()),
        Ok(None) => FfiResult::binary(b"".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Checks if a handle is permanently claimed (verified) by any peer.
#[no_mangle]
pub extern "C" fn introvert_storage_is_handle_claimed(handle_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if handle_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let handle = unsafe { CStr::from_ptr(handle_ptr).to_string_lossy().into_owned() };

    match engine.storage.is_handle_permanently_claimed(&handle) {
        Ok(claimed) => FfiResult::binary(vec![if claimed { 1 } else { 0 }]),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Registers a mobile push token with the RBN backbone for background wakeups.
#[no_mangle]
pub extern "C" fn introvert_network_register_push_token(
    device_type_ptr: *const c_char,
    token_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if device_type_ptr.is_null() || token_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let device_type = unsafe { CStr::from_ptr(device_type_ptr).to_string_lossy().into_owned() };
    let push_token = unsafe { CStr::from_ptr(token_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    // Save token to local DB so we can auto-register on reconnect
    let db_token = push_token.clone();
    let db_device = device_type.clone();
    let local_peer = engine.identity.peer_id.to_string();
    let storage = Arc::clone(&engine.storage);
    match storage.save_push_token(&local_peer, &db_device, &db_token) {
        Ok(_) => dispatch_debug_log(&format!("FFI: Push token saved successfully to local DB under key: {}", local_peer)),
        Err(e) => dispatch_debug_log(&format!("FFI: ❌ Failed to save push token to local DB: {:?}", e)),
    }

    engine.runtime.spawn(async move {
        let payload = crate::network::SignalingPayload::IdentifySleepState { device_type, push_token };
        // Broadcast to all RBNs/Bootstrap nodes
        let bootstrap = crate::network::config::get_bootstrap_nodes();
        for (pid, _) in bootstrap {
            let _ = tx.send(crate::network::NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
        }
    });

    FfiResult::success()
}
/// Sends an emoji reaction to a message.
#[no_mangle]
pub extern "C" fn introvert_network_send_reaction(
    target_id_ptr: *const c_char, // PeerID or GroupID
    msg_id_ptr: *const c_char,
    emoji_ptr: *const c_char,
    is_group: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if target_id_ptr.is_null() || msg_id_ptr.is_null() || emoji_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let target_id_str = unsafe { CStr::from_ptr(target_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    let emoji = unsafe { CStr::from_ptr(emoji_ptr).to_string_lossy().into_owned() };

    let my_peer_id = engine.identity.peer_id.to_string();
    if let Err(e) = engine.storage.add_message_reaction(&msg_id, &my_peer_id, &emoji) {
        return FfiResult::error(-1, &format!("Storage error: {}", e));
    }

    // Record daily reward activity for reaction
    if let Some(ref daily) = engine.daily_reward_engine {
        daily.record_activity(crate::economy::daily_rewards::ActivityEvent {
            activity_type: crate::economy::daily_rewards::ActivityType::GroupReaction,
            peer_id: Some(target_id_str.clone()),
            value: 1,
            is_foreground: true,
            message_len: None,
            is_self: false,
            is_rbn: false,
            proof_hash: None,
        });
    }

    // DISPATCH LOCALLY: Ensure sender's UI updates immediately
    let mut local_data = vec![msg_id.len() as u8];
    local_data.extend(msg_id.as_bytes());
    local_data.extend(emoji.as_bytes());
    crate::dispatch_global_event(35, &local_data);

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if is_group {
        let action = crate::network::GroupAction::Reaction { msg_id, emoji };
        let signed = match crate::network::group::GroupManager::sign_action(target_id_str.clone(), action, &engine.identity.keypair) {
            Ok(s) => s,
            Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
        };
        let payload = crate::network::SignalingPayload::GroupAction(signed);
        let storage = engine.storage.clone();
        engine.runtime.spawn(async move {
            if let Ok(Some(members_json)) = storage.get_group_members(&target_id_str) {
                let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                for m in members {
                    if m.peer_id == my_peer_id { continue; }
                    if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                    }
                }
            }
        });
    } else {
        if let Ok(peer_id) = PeerId::from_str(&target_id_str) {
            engine.runtime.spawn(async move {
                let payload = crate::network::SignalingPayload::MessageReaction { msg_id, emoji };
                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
            });
        }
    }

    FfiResult::success()
}

/// Retrieves aggregated reactions for a message.
#[no_mangle]
pub extern "C" fn introvert_storage_get_reactions(
    msg_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if msg_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };

    match engine.storage.get_message_reactions(&msg_id) {
        Ok(json) => FfiResult::binary(json.to_string().into_bytes()),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

/// Sends a direct connection invite to a peer.
#[no_mangle]
pub extern "C" fn introvert_network_send_direct_invite(
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

    let identity = Arc::clone(&engine.identity);
    let storage = Arc::clone(&engine.storage);
    
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

    let (local_name, local_handle, local_avatar, _, local_tier) = storage.get_profile().unwrap_or(None).unwrap_or((None, None, None, 0, 0));

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        p2p_pubkey: identity.keypair.public().encode_protobuf(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        global_name: local_name.clone(),
        local_alias: local_name,
        avatar_base64: local_avatar,
        is_anchor_capable: true, 
        retention_seconds: 0,
        handle: local_handle,
        prestige_tier: Some(local_tier as u8),
    };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    let status = storage.get_contact_status(&peer_id_str).ok().flatten();
    let is_accept = match status {
        Some((false, true)) => {
            let _ = storage.update_contact_verification(&peer_id_str, true);
            true
        }
        _ => {
            if status.is_none() {
                let placeholder = crate::identity::SovereignIdentity {
                    peer_id: peer_id_str.clone(),
                    p2p_pubkey: vec![],
                    static_key: [0u8; 32],
                    solana_address: "".to_string(),
                    global_name: None,
                    local_alias: None,
                    avatar_base64: None,
                    is_anchor_capable: false,
                    retention_seconds: 0,
                    handle: None,
                    prestige_tier: None,
                };
                let _ = storage.upsert_sovereign_contact(&placeholder, false, false);
            }
            false
        }
    };

    engine.runtime.spawn(async move {
        let _ = tx.send(crate::network::NetworkCommand::SendDirectInvite { 
            peer_id, 
            identity: my_identity,
            is_accept,
        }).await;
    });

    FfiResult::success()
}

/// Sets retention policy and gossips it to the peer/group.
#[no_mangle]
pub extern "C" fn introvert_network_set_retention(
    target_id_ptr: *const c_char,
    seconds: u32,
    is_group: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if target_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let target_id_str = unsafe { CStr::from_ptr(target_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if is_group {
        let _ = engine.storage.set_group_retention(&target_id_str, seconds);
        let action = crate::network::GroupAction::SetRetention { seconds };
        let signed = match crate::network::group::GroupManager::sign_action(target_id_str.clone(), action, &engine.identity.keypair) {
            Ok(s) => s,
            Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
        };
        let payload = crate::network::SignalingPayload::GroupAction(signed);
        let storage = engine.storage.clone();
        let my_peer_id = engine.identity.peer_id.to_string();
        engine.runtime.spawn(async move {
            if let Ok(Some(members_json)) = storage.get_group_members(&target_id_str) {
                let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                for m in members {
                    if m.peer_id == my_peer_id { continue; }
                    if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                    }
                }
            }
        });
    } else {
        let _ = engine.storage.set_contact_retention(&target_id_str, seconds);
        if let Ok(peer_id) = PeerId::from_str(&target_id_str) {
            engine.runtime.spawn(async move {
                let payload = crate::network::SignalingPayload::SetRetention { seconds };
                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
            });
        }
    }

    FfiResult::success()
}

/// Deletes a message locally and gossips the deletion.
#[no_mangle]
pub extern "C" fn introvert_network_delete_message(
    target_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
    is_group: bool,
    deleted_by_admin: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if target_id_ptr.is_null() || msg_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let target_id_str = unsafe { CStr::from_ptr(target_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };

    let _ = engine.storage.delete_message(&msg_id, is_group, deleted_by_admin);

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if is_group {
        let action = crate::network::GroupAction::DeleteMessage { msg_id };
        let signed = match crate::network::group::GroupManager::sign_action(target_id_str.clone(), action, &engine.identity.keypair) {
            Ok(s) => s,
            Err(e) => return FfiResult::error(-4, &format!("Sign error: {}", e)),
        };
        let payload = crate::network::SignalingPayload::GroupAction(signed);
        let storage = engine.storage.clone();
        let my_peer_id = engine.identity.peer_id.to_string();
        engine.runtime.spawn(async move {
            if let Ok(Some(members_json)) = storage.get_group_members(&target_id_str) {
                let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                for m in members {
                    if m.peer_id == my_peer_id { continue; }
                    if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                    }
                }
            }
        });
    } else {
        if let Ok(peer_id) = PeerId::from_str(&target_id_str) {
            engine.runtime.spawn(async move {
                let payload = crate::network::SignalingPayload::DeleteMessage { msg_id };
                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
            });
        }
    }

    FfiResult::success()
}

/// Edits a message locally and gossips the edit.
#[no_mangle]
pub extern "C" fn introvert_network_edit_message(
    target_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
    new_content_ptr: *const c_char,
    is_group: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if target_id_ptr.is_null() || msg_id_ptr.is_null() || new_content_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let target_id_str = unsafe { CStr::from_ptr(target_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    let new_content = unsafe { CStr::from_ptr(new_content_ptr).to_string_lossy().into_owned() };

    let _ = engine.storage.edit_message(&msg_id, &new_content, is_group);

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if is_group {
        let storage = engine.storage.clone();
        let my_peer_id = engine.identity.peer_id.to_string();
        let keypair = engine.identity.keypair.clone();
        engine.runtime.spawn(async move {
            if let Ok(Some(group_info)) = storage.get_group(&target_id_str) {
                use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                use rand::RngCore;
                let mut nonce_bytes = [0u8; 12];
                rand::thread_rng().fill_bytes(&mut nonce_bytes);
                let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                if let Ok(encrypted) = cipher.encrypt(Nonce::from_slice(&nonce_bytes), new_content.as_bytes()) {
                    let mut new_content_encrypted = nonce_bytes.to_vec();
                    new_content_encrypted.extend(encrypted);
                    let action = crate::network::GroupAction::EditMessage { msg_id, new_content_encrypted };
                    if let Ok(signed) = crate::network::group::GroupManager::sign_action(target_id_str.clone(), action, &keypair) {
                        let payload = crate::network::SignalingPayload::GroupAction(signed);
                        if let Ok(Some(members_json)) = storage.get_group_members(&target_id_str) {
                            let members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                            for m in members {
                                if m.peer_id == my_peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: pid, payload: payload.clone() }).await;
                                }
                            }
                        }
                    }
                }
            }
        });
    } else {
        if let Ok(peer_id) = PeerId::from_str(&target_id_str) {
            engine.runtime.spawn(async move {
                let payload = crate::network::SignalingPayload::EditMessage { msg_id, new_content };
                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
            });
        }
    }

    FfiResult::success()
}

/// Polls the profile of a specific peer by sending a ProfileRequest.
#[no_mangle]
pub extern "C" fn introvert_network_poll_peer_profile(
    peer_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if let Ok(peer_id) = libp2p::PeerId::from_str(&peer_id_str) {
        engine.runtime.spawn(async move {
            let _ = tx.send(crate::network::NetworkCommand::PollPeerProfile { peer_id }).await;
        });
    }

    FfiResult::success()
}

// ==================== MESSAGE SYNC FFI ====================

/// Triggers message sync with a peer. Sends a ChatSyncRequest to the peer
/// who responds with any messages the local device is missing.
/// is_full: if 1, sends empty known_msg_ids to request ALL messages (full history sync)
#[no_mangle]
pub extern "C" fn introvert_network_sync_chat_messages(
    peer_id_ptr: *const c_char,
    chat_id_ptr: *const c_char,
    is_group: i32,
    is_full: i32,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if peer_id_ptr.is_null() || chat_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }

    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let chat_id = unsafe { CStr::from_ptr(chat_id_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() {
        Some(t) => t.clone(),
        None => return FfiResult::error(-13, "Network not started"),
    };

    if let Ok(peer_id) = libp2p::PeerId::from_str(&peer_id_str) {
        engine.runtime.spawn(async move {
            let _ = tx.send(crate::network::NetworkCommand::SyncChatMessages {
                peer_id,
                chat_id,
                is_group: is_group != 0,
                is_full: is_full != 0,
            }).await;
        });
    }

    FfiResult::success()
}

// ==================== NOTES FFI ====================

#[no_mangle]
pub extern "C" fn introvert_notes_create(
    id_ptr: *const c_char,
    title_ptr: *const c_char,
    content_ptr: *const c_char,
    tags_ptr: *const c_char,
    image_path_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if id_ptr.is_null() || title_ptr.is_null() || content_ptr.is_null() || tags_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let id = unsafe { CStr::from_ptr(id_ptr).to_string_lossy().into_owned() };
    let title = unsafe { CStr::from_ptr(title_ptr).to_string_lossy().into_owned() };
    let content = unsafe { CStr::from_ptr(content_ptr).to_string_lossy().into_owned() };
    let tags = unsafe { CStr::from_ptr(tags_ptr).to_string_lossy().into_owned() };
    let image_path = if image_path_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(image_path_ptr).to_string_lossy().into_owned() }) };
    match engine.storage.create_note(&id, &title, &content, &tags, image_path.as_deref()) {
        Ok(_) => FfiResult::success(), Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_update(
    id_ptr: *const c_char, title_ptr: *const c_char, content_ptr: *const c_char,
    tags_ptr: *const c_char, image_path_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if id_ptr.is_null() || title_ptr.is_null() || content_ptr.is_null() || tags_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let id = unsafe { CStr::from_ptr(id_ptr).to_string_lossy().into_owned() };
    let title = unsafe { CStr::from_ptr(title_ptr).to_string_lossy().into_owned() };
    let content = unsafe { CStr::from_ptr(content_ptr).to_string_lossy().into_owned() };
    let tags = unsafe { CStr::from_ptr(tags_ptr).to_string_lossy().into_owned() };
    let image_path = if image_path_ptr.is_null() { None } else { Some(unsafe { CStr::from_ptr(image_path_ptr).to_string_lossy().into_owned() }) };
    match engine.storage.update_note(&id, &title, &content, &tags, image_path.as_deref()) {
        Ok(_) => FfiResult::success(), Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_delete(id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let id = unsafe { CStr::from_ptr(id_ptr).to_string_lossy().into_owned() };
    match engine.storage.delete_note(&id) {
        Ok(_) => FfiResult::success(), Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_get(id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let id = unsafe { CStr::from_ptr(id_ptr).to_string_lossy().into_owned() };
    match engine.storage.get_note(&id) {
        Ok(Some((id, title, content, tags, image_path, created_at, updated_at))) => {
            let json = serde_json::json!({ "id": id, "title": title, "content": content, "tags": tags, "image_path": image_path, "created_at": created_at, "updated_at": updated_at });
            FfiResult::binary(serde_json::to_vec(&json).unwrap_or_default())
        }
        Ok(None) => FfiResult::error(-2, "Note not found"),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_get_all() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    match engine.storage.get_all_notes() {
        Ok(notes) => {
            let json_notes: Vec<serde_json::Value> = notes.into_iter().map(|(id, title, content, tags, image_path, created_at, updated_at)| {
                serde_json::json!({ "id": id, "title": title, "content": content, "tags": tags, "image_path": image_path, "created_at": created_at, "updated_at": updated_at })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_notes).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_search(query_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if query_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let query = unsafe { CStr::from_ptr(query_ptr).to_string_lossy().into_owned() };
    match engine.storage.search_notes(&query) {
        Ok(notes) => {
            let json_notes: Vec<serde_json::Value> = notes.into_iter().map(|(id, title, content, tags, image_path, created_at, updated_at)| {
                serde_json::json!({ "id": id, "title": title, "content": content, "tags": tags, "image_path": image_path, "created_at": created_at, "updated_at": updated_at })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_notes).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_save_version(note_id_ptr: *const c_char, title_ptr: *const c_char, content_ptr: *const c_char, tags_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if note_id_ptr.is_null() || title_ptr.is_null() || content_ptr.is_null() || tags_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let note_id = unsafe { CStr::from_ptr(note_id_ptr).to_string_lossy().into_owned() };
    let title = unsafe { CStr::from_ptr(title_ptr).to_string_lossy().into_owned() };
    let content = unsafe { CStr::from_ptr(content_ptr).to_string_lossy().into_owned() };
    let tags = unsafe { CStr::from_ptr(tags_ptr).to_string_lossy().into_owned() };
    match engine.storage.save_note_version(&note_id, &title, &content, &tags) {
        Ok(version) => FfiResult::binary(version.to_string().into_bytes()),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_notes_get_versions(note_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if note_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let note_id = unsafe { CStr::from_ptr(note_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.get_note_versions(&note_id) {
        Ok(versions) => {
            let json_versions: Vec<serde_json::Value> = versions.into_iter().map(|(num, title, content, tags, created_at)| {
                serde_json::json!({ "version": num, "title": title, "content": content, "tags": tags, "created_at": created_at })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_versions).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

// ==================== CALL HISTORY FFI ====================

#[no_mangle]
pub extern "C" fn introvert_call_history_log(
    peer_id_ptr: *const c_char,
    call_type_ptr: *const c_char,
    media_type: i32,
    duration_seconds: i32,
    is_incoming: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if peer_id_ptr.is_null() || call_type_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let call_type = unsafe { CStr::from_ptr(call_type_ptr).to_string_lossy().into_owned() };
    match engine.storage.log_call(&peer_id, &call_type, media_type, duration_seconds, is_incoming) {
        Ok(_) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_call_history_get(limit: i32) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    let limit = limit.clamp(0, 10000);
    match engine.storage.get_call_history(limit) {
        Ok(history) => {
            let json_history: Vec<serde_json::Value> = history.into_iter().map(|(peer_id, call_type, media_type, duration, is_incoming, timestamp)| {
                serde_json::json!({
                    "peer_id": peer_id, "call_type": call_type, "media_type": media_type,
                    "duration_seconds": duration, "is_incoming": is_incoming, "timestamp": timestamp
                })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_history).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_call_history_count() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    match engine.storage.get_call_count() {
        Ok(count) => FfiResult::binary(count.to_string().into_bytes()),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

// ==================== MESSAGE SEARCH FFI ====================

#[no_mangle]
pub extern "C" fn introvert_search_messages(peer_id_ptr: *const c_char, query_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if peer_id_ptr.is_null() || query_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let query = unsafe { CStr::from_ptr(query_ptr).to_string_lossy().into_owned() };
    match engine.storage.search_messages(&peer_id, &query) {
        Ok(messages) => {
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(content, timestamp, is_me, status, msg_id, reply_to)| {
                serde_json::json!({
                    "content": content, "timestamp": timestamp, "is_me": is_me,
                    "status": status, "msg_id": msg_id, "reply_to": reply_to
                })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_messages).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_search_group_messages(group_id_ptr: *const c_char, query_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if group_id_ptr.is_null() || query_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let group_id = unsafe { CStr::from_ptr(group_id_ptr).to_string_lossy().into_owned() };
    let query = unsafe { CStr::from_ptr(query_ptr).to_string_lossy().into_owned() };
    match engine.storage.search_group_messages(&group_id, &query) {
        Ok(messages) => {
            let json_messages: Vec<serde_json::Value> = messages.into_iter().map(|(sender_id, msg_id, content, timestamp, reply_to)| {
                serde_json::json!({
                    "sender_id": sender_id, "msg_id": msg_id, "content": content,
                    "timestamp": timestamp, "reply_to": reply_to
                })
            }).collect();
            FfiResult::binary(serde_json::to_vec(&json_messages).unwrap_or_default())
        }
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

// ==================== TYPING INDICATOR & LAST SEEN FFI ====================

#[no_mangle]
pub extern "C" fn introvert_send_typing_start(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id = match PeerId::from_str(&peer_id_str) { Ok(pid) => pid, Err(_) => return FfiResult::error(-12, "Invalid PeerId") };
    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() { Some(t) => t.clone(), None => return FfiResult::error(-13, "Network not started") };
    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: crate::network::SignalingPayload::TypingStart { chat_id: peer_id_str } }).await;
    });
    FfiResult::success()
}

#[no_mangle]
pub extern "C" fn introvert_send_typing_stop(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    let peer_id = match PeerId::from_str(&peer_id_str) { Ok(pid) => pid, Err(_) => return FfiResult::error(-12, "Invalid PeerId") };
    let tx_lock = engine.network_tx.read();
    let tx = match tx_lock.as_ref() { Some(t) => t.clone(), None => return FfiResult::error(-13, "Network not started") };
    engine.runtime.spawn(async move {
        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: crate::network::SignalingPayload::TypingStop { chat_id: peer_id_str } }).await;
    });
    FfiResult::success()
}

#[no_mangle]
pub extern "C" fn introvert_get_last_seen(peer_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if peer_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let peer_id_str = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.get_last_seen(&peer_id_str) {
        Ok(Some(ts)) => FfiResult::binary(ts.to_string().into_bytes()),
        Ok(None) => FfiResult::binary(b"0".to_vec()),
        Err(e) => FfiResult::error(-1, &format!("Database error: {}", e)),
    }
}

// ── Elevated Messages ──────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn introvert_elevate_message(
    chat_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
    content_ptr: *const c_char,
    sender_id_ptr: *const c_char,
    is_me: bool,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if chat_id_ptr.is_null() || msg_id_ptr.is_null() || content_ptr.is_null() || sender_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }
    let chat_id = unsafe { CStr::from_ptr(chat_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    let content = unsafe { CStr::from_ptr(content_ptr).to_string_lossy().into_owned() };
    let sender_id = unsafe { CStr::from_ptr(sender_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.elevate_message(&chat_id, &msg_id, &content, &sender_id, is_me) {
        Ok(()) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_unelevate_message(
    chat_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if chat_id_ptr.is_null() || msg_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }
    let chat_id = unsafe { CStr::from_ptr(chat_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.unelevate_message(&chat_id, &msg_id) {
        Ok(()) => FfiResult::success(),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_get_elevated_messages(chat_id_ptr: *const c_char) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if chat_id_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let chat_id = unsafe { CStr::from_ptr(chat_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.get_elevated_messages(&chat_id) {
        Ok(val) => {
            let json = serde_json::to_string(&val).unwrap_or_else(|_| "[]".to_string());
            FfiResult::binary(json.into_bytes())
        }
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn introvert_is_message_elevated(
    chat_id_ptr: *const c_char,
    msg_id_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if chat_id_ptr.is_null() || msg_id_ptr.is_null() {
        return FfiResult::error(-11, "Null pointer");
    }
    let chat_id = unsafe { CStr::from_ptr(chat_id_ptr).to_string_lossy().into_owned() };
    let msg_id = unsafe { CStr::from_ptr(msg_id_ptr).to_string_lossy().into_owned() };
    match engine.storage.is_message_elevated(&chat_id, &msg_id) {
        Ok(elevated) => FfiResult::binary(if elevated { b"1".to_vec() } else { b"0".to_vec() }),
        Err(e) => FfiResult::error(-1, &format!("Storage error: {}", e)),
    }
}

// ── Daily Rewards FFI ─────────────────────────────────────────

#[no_mangle]
pub extern "C" fn introvert_daily_reward_get_status() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    match &engine.daily_reward_engine {
        Some(daily) => FfiResult::binary(daily.get_status_json().into_bytes()),
        None => FfiResult::binary(b"{}".to_vec()),
    }
}

#[no_mangle]
pub extern "C" fn introvert_daily_reward_get_history(days: u32) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    match &engine.daily_reward_engine {
        Some(daily) => FfiResult::binary(daily.get_history_json(days).into_bytes()),
        None => FfiResult::binary(b"[]".to_vec()),
    }
}

#[no_mangle]
pub extern "C" fn introvert_daily_reward_record_activity(
    json_ptr: *const u8,
    json_len: usize,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if json_ptr.is_null() || json_len == 0 {
        return FfiResult::error(-11, "Null pointer");
    }
    let daily = match &engine.daily_reward_engine {
        Some(d) => d,
        None => return FfiResult::error(-12, "Daily rewards not initialized"),
    };
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-13, &format!("Invalid UTF-8: {}", e)),
    };
    let event: crate::economy::daily_rewards::ActivityEvent = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => return FfiResult::error(-14, &format!("Invalid JSON: {}", e)),
    };
    let accepted = daily.record_activity(event);
    FfiResult::binary(if accepted { b"1".to_vec() } else { b"0".to_vec() })
}

#[no_mangle]
pub extern "C" fn introvert_daily_reward_update_weights(
    json_ptr: *const u8,
    json_len: usize,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if json_ptr.is_null() || json_len == 0 {
        return FfiResult::error(-11, "Null pointer");
    }
    let daily = match &engine.daily_reward_engine {
        Some(d) => d,
        None => return FfiResult::error(-12, "Daily rewards not initialized"),
    };
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-13, &format!("Invalid UTF-8: {}", e)),
    };
    let weights: crate::economy::daily_rewards::ActivityWeights = match serde_json::from_str(json_str) {
        Ok(w) => w,
        Err(e) => return FfiResult::error(-14, &format!("Invalid JSON: {}", e)),
    };
    daily.update_weights(weights);
    FfiResult::success()
}

#[no_mangle]
pub extern "C" fn introvert_daily_reward_update_anti_gaming(
    json_ptr: *const u8,
    json_len: usize,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    if json_ptr.is_null() || json_len == 0 {
        return FfiResult::error(-11, "Null pointer");
    }
    let daily = match &engine.daily_reward_engine {
        Some(d) => d,
        None => return FfiResult::error(-12, "Daily rewards not initialized"),
    };
    let json_bytes = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_bytes) {
        Ok(s) => s,
        Err(e) => return FfiResult::error(-13, &format!("Invalid UTF-8: {}", e)),
    };
    let config: crate::economy::daily_rewards::AntiGamingConfig = match serde_json::from_str(json_str) {
        Ok(c) => c,
        Err(e) => return FfiResult::error(-14, &format!("Invalid JSON: {}", e)),
    };
    daily.update_anti_gaming(config);
    FfiResult::success()
}

#[no_mangle]
pub extern "C" fn introvert_daily_reward_get_realtime_earnings() -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() { Some(e) => e, None => return FfiResult::error(-10, "Engine not started") };
    match &engine.daily_reward_engine {
        Some(daily) => {
            let earnings = daily.get_realtime_earnings();
            let json_str = serde_json::to_string(&earnings).unwrap_or_else(|_| "{}".to_string());
            FfiResult::binary(json_str.into_bytes())
        }
        None => FfiResult::binary(b"{}".to_vec()),
    }
}

/// Returns a JSON list of all registered RBN bootstrap nodes, including address and ping latency.
#[no_mangle]
pub extern "C" fn introvert_network_get_rbns() -> FfiResult {
    let bootstrap = crate::BOOTSTRAP_NODES.read();
    let latencies = crate::RBN_LATENCIES.read();

    let mut list = Vec::new();
    for (pid, addr) in bootstrap.iter() {
        let latency = latencies.get(pid).cloned();
        list.push(json!({
            "peer_id": pid,
            "address": addr,
            "latency_ms": latency,
        }));
    }

    match serde_json::to_string(&list) {
        Ok(s) => FfiResult::binary(s.into_bytes()),
        Err(_) => FfiResult::error(-1, "JSON serialization failed"),
    }
}

/// Triggers a connection test to a manual RBN IP/Multiaddress.
/// When finished, confirms via Event 45 (RbnConnectionConfirmed) or Event 46 (RbnConnectionFailed).
#[no_mangle]
pub extern "C" fn introvert_network_test_rbn(
    address_ptr: *const c_char,
) -> FfiResult {
    let lock = ENGINE.read();
    let engine = match lock.as_ref() {
        Some(e) => e,
        None => return FfiResult::error(-10, "Engine not started"),
    };

    if address_ptr.is_null() { return FfiResult::error(-11, "Null pointer"); }
    let address = unsafe { CStr::from_ptr(address_ptr).to_string_lossy().into_owned() };

    let tx_lock = engine.network_tx.read();
    if let Some(ref tx) = *tx_lock {
        let tx_clone = tx.clone();
        engine.runtime.spawn(async move {
            let _ = tx_clone.send(NetworkCommand::TestManualRbn { address }).await;
        });
        FfiResult::success()
    } else {
        FfiResult::error(-12, "Network not started")
    }
}
