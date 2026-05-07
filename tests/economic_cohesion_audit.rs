use std::ffi::{CStr, CString};
use std::time::Duration;
use introvert::*;
use introvert::identity::NodeIdentity;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;
use once_cell::sync::Lazy;

static LAST_STATS: Lazy<Mutex<Option<serde_json::Value>>> = Lazy::new(|| Mutex::new(None));

extern "C" fn economy_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 9 {
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        if let Ok(stats) = serde_json::from_slice::<serde_json::Value>(data_slice) {
            let mut lock = LAST_STATS.lock();
            *lock = Some(stats);
        }
    }
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[test]
fn test_economic_cohesion_audit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("cohesion_audit.db");
    let db_path_c = CString::new(db_path.to_str().unwrap()).unwrap();

    // --- 1. Key Derivation Check ---
    println!("🔐 Phase 1: Key Derivation Audit...");
    
    let mnemonic_ptr = introvert_generate_mnemonic();
    let mnemonic_str = unsafe { CStr::from_ptr(mnemonic_ptr).to_string_lossy().into_owned() };
    println!("Mnemonic: {}", mnemonic_str);

    let seed_res = introvert_mnemonic_to_seed(mnemonic_ptr);
    assert_eq!(seed_res.code, 0);
    let mut seed = [0u8; 32];
    unsafe { std::ptr::copy_nonoverlapping(seed_res.data, seed.as_mut_ptr(), 32); }
    introvert_free_binary(seed_res.data, seed_res.len);
    introvert_free_string(mnemonic_ptr);

    // Derive expected Solana address using core logic
    let solana_signing_key = NodeIdentity::derive_solana_keypair(seed).expect("Failed to derive key");
    let expected_address = solana_sdk::pubkey::Pubkey::new_from_array(solana_signing_key.verifying_key().to_bytes()).to_string();
    println!("Expected Solana Address: {}", expected_address);

    // Start Engine
    let res = introvert_engine_start(seed.as_ptr(), db_path_c.as_ptr());
    assert_eq!(res.code, 0);

    *introvert::TEST_CALLBACK.write() = Some(economy_callback);

    // Start Economy Monitoring
    let res = introvert_economy_start_monitoring(economy_callback);
    assert_eq!(res.code, 0);

    // Wait for first update
    println!("Waiting for economy update...");
    let mut reported_address = String::new();
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(1));
        let lock = LAST_STATS.lock();
        if let Some(stats) = lock.as_ref() {
            reported_address = stats["sol_address"].as_str().unwrap_or_default().to_string();
            break;
        }
    }

    println!("Reported Solana Address: {}", reported_address);
    assert_eq!(expected_address, reported_address, "CRITICAL: Solana address mismatch in UI/FFI!");
    println!("✅ Key Derivation Audit PASSED.");

    // --- 2. Proof Accuracy Check ---
    println!("\n📑 Phase 2: Proof Accuracy Audit...");
    
    // Access internal tracker to simulate activity
    // Note: We'll use a hack to record relay since we are in the same binary for tests
    // In a real audit, we'd trigger network activity.
    
    // For the sake of this specific requirement "Confirm that the signed 'Work Proof' matches the byte-count",
    // we will directly test the RewardTracker logic with a known consumer.
    
    let storage = Arc::new(introvert::storage::StorageService::new(db_path, &NodeIdentity::derive_storage_key(seed).unwrap()).unwrap());
    let tracker = introvert::economy::RewardTracker::new(Some(storage));
    
    let consumer_id = "12D3KooWTEST_CONSUMER_PEER_ID";
    let bytes_to_relay = 10 * 1024 * 1024; // 10 MB
    
    println!("Recording relay of {} bytes for consumer {}...", bytes_to_relay, consumer_id);
    tracker.record_relay(consumer_id, bytes_to_relay);
    
    let (amount, proof_bytes) = tracker.prepare_reward_proof(&expected_address, consumer_id)
        .expect("Failed to prepare reward proof (threshold/cooldown?)");
    
    assert_eq!(amount, bytes_to_relay, "Proof amount mismatch with recorded bytes!");
    
    let proof: introvert::economy::RewardProof = serde_json::from_slice(&proof_bytes).expect("Failed to deserialize proof");
    println!("Generated Proof: {:?}", proof);
    
    assert_eq!(proof.provider_pubkey, expected_address);
    assert_eq!(proof.consumer_peer_id, consumer_id);
    assert_eq!(proof.relayed_bytes, bytes_to_relay);
    
    println!("✅ Proof Accuracy Audit PASSED.");

    introvert_engine_stop();
}
