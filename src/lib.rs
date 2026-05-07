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
        let mut data = msg.as_bytes().to_vec();
        data.shrink_to_fit();
        let len = data.len();
        let ptr = data.as_ptr() as *mut u8;
        std::mem::forget(data);
        Self { code, data: ptr, len }
    }

    pub fn binary(mut data: Vec<u8>) -> Self {
        data.shrink_to_fit();
        let len = data.len();
        let ptr = data.as_ptr() as *mut u8;
        std::mem::forget(data);
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
}

pub static ENGINE: Lazy<RwLock<Option<Engine>>> = Lazy::new(|| RwLock::new(None));

pub static TEST_CALLBACK: Lazy<RwLock<Option<FfiNetworkCallback>>> = Lazy::new(|| RwLock::new(None));

/// Dispatches an event to the global FFI callback stored in ENGINE or TEST_CALLBACK.
pub fn dispatch_global_event(event_type: i32, data_ptr: *const u8, data_len: usize) {
    println!("FFI Dispatch: event={}, len={}", event_type, data_len);
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

    // Initialize Solana Incentive Engine with Devnet settings as default
    let solana_client = match SolanaIncentiveEngine::new(
        "https://api.devnet.solana.com",
        "F7wNqXTRyHpKtx9BZEWVefUyf3wqTVw4mAqK2HafNU94", // Authority from MEMORY.md
        "https://api.introvert.network/v1/treasury/claim" // Hypothetical Treasury Relay
    ) {
        Ok(c) => Arc::new(c),
        Err(_) => return FfiResult::error(-7, "Solana client initialization failed"),
    };

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(_) => return FfiResult::error(-6, "Runtime initialization failed"),
    };

    let mut engine_lock = ENGINE.write();
    *engine_lock = Some(Engine {
        runtime,
        identity,
        storage,
        reward_tracker,
        solana_client,
        session_encryption_key,
        network_tx: RwLock::new(None),
        network_callback: RwLock::new(None),
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

    // Check if network is already running
    {
        let tx_lock = engine.network_tx.read();
        if tx_lock.is_some() {
            println!("Network already running. Callback updated.");
            return FfiResult::success();
        }
    }

    let keypair = engine.identity.keypair.clone();
    let storage = Arc::clone(&engine.storage);
    let reward_tracker = Arc::clone(&engine.reward_tracker);
    let (tx, rx) = mpsc::channel(100);
    
    // Derive the E2EE key for Noise sessions
    let local_static_secret = match NodeIdentity::derive_e2ee_key(engine.identity.seed) {
        Ok(k) => k,
        Err(_) => return FfiResult::error(-14, "E2EE key derivation failed"),
    };

    let session_encryption_key = engine.session_encryption_key;

    // Store the sender in the engine state
    {
        let mut tx_lock = engine.network_tx.write();
        *tx_lock = Some(tx);
    }
    
    engine.runtime.spawn(async move {
        match NetworkService::new(keypair, callback, rx, storage, reward_tracker, local_static_secret, session_encryption_key, true, true, tcp_port, enable_relay_server).await {
            Ok(service) => {
                service.run().await;
            }
            Err(e) => {
                eprintln!("Failed to start network service: {}", e);
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
                let mut data = stats_str.into_bytes();
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                dispatch_global_event(9, ptr, len);
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
                let mut data = stats_str.into_bytes();
                data.shrink_to_fit();
                let ptr = data.as_ptr();
                let len = data.len();
                std::mem::forget(data);
                dispatch_global_event(9, ptr, len);
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
        
        let user_keypair = solana_sdk::signature::Keypair::from_bytes(&solana_signing_key.to_bytes()).unwrap();

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
        match tx.send(NetworkCommand::SendSignaling { peer_id, message }).await {
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
            let json = serde_json::to_string(&contacts).unwrap_or_default();
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
            storage.store_message(&peer_id, &msg)
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

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        is_anchor_capable: true, 
    };

    engine.runtime.spawn(async move {
        match crate::network::wormhole::create_invite(my_identity).await {
            Ok((code, handshake_future)) => {
                // Emit the code to the UI (Event Type 6)
                let mut code_bytes = code.into_bytes();
                code_bytes.shrink_to_fit();
                let ptr = code_bytes.as_ptr();
                let len = code_bytes.len();
                std::mem::forget(code_bytes);
                dispatch_global_event(6, ptr, len);
                
                // Wait for the peer to connect and exchange identity
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        let mut peer_id_bytes = peer_identity.peer_id.into_bytes();
                        peer_id_bytes.shrink_to_fit();
                        let ptr = peer_id_bytes.as_ptr();
                        let len = peer_id_bytes.len();
                        std::mem::forget(peer_id_bytes);
                        dispatch_global_event(7, ptr, len);
                    }
                    Err(e) => eprintln!("Wormhole handshake failed: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to create Wormhole invite: {}", e),
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

    let my_identity = crate::identity::SovereignIdentity {
        peer_id: identity.peer_id.to_string(),
        static_key: local_static_public.to_bytes(),
        solana_address,
        is_anchor_capable: true, 
    };

    engine.runtime.spawn(async move {
        match crate::network::wormhole::accept_invite(code, my_identity).await {
            Ok(handshake_future) => {
                match handshake_future.await {
                    Ok(peer_identity) => {
                        let _ = storage.upsert_sovereign_contact(&peer_identity);
                        // Emit a 'Handover Complete' event (Event Type 7)
                        let mut peer_id_bytes = peer_identity.peer_id.into_bytes();
                        peer_id_bytes.shrink_to_fit();
                        let ptr = peer_id_bytes.as_ptr();
                        let len = peer_id_bytes.len();
                        std::mem::forget(peer_id_bytes);
                        dispatch_global_event(7, ptr, len);
                    }
                    Err(e) => eprintln!("Wormhole join handshake failed: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to join Wormhole session: {}", e),
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
        let msg = "renegotiation_started";
        dispatch_global_event(0, msg.as_ptr(), msg.len());
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
pub extern "C" fn introvert_free_binary(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, len, len);
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
