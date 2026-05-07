use std::ffi::{CStr, CString};
use std::time::Duration;
use introvert::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static EVENT_9_COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn economy_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 9 {
        EVENT_9_COUNT.fetch_add(1, Ordering::SeqCst);
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        let data_str = String::from_utf8_lossy(data_slice);
        println!("Economy Update Received: {}", data_str);
    }
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

extern "C" fn reward_callback(status: i32, tx_signature_ptr: *const libc::c_char) {
    let sig = if !tx_signature_ptr.is_null() {
        unsafe { CStr::from_ptr(tx_signature_ptr).to_string_lossy().into_owned() }
    } else {
        "None".to_string()
    };
    println!("Reward Claim Result: status={}, sig={}", status, sig);
    if !tx_signature_ptr.is_null() {
        introvert_free_string(tx_signature_ptr as *mut _);
    }
}

#[test]
fn test_economy_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("economy_test.db");
    let db_path_c = CString::new(db_path.to_str().unwrap()).unwrap();

    let seed = [1u8; 32];
    
    // 1. Start Engine
    let res = introvert_engine_start(seed.as_ptr(), db_path_c.as_ptr());
    assert_eq!(res.code, 0);

    *introvert::TEST_CALLBACK.write() = Some(economy_callback);

    // 2. Start Economy Monitoring
    let res = introvert_economy_start_monitoring(economy_callback);
    assert_eq!(res.code, 0);

    // 3. Simulate some relay activity (Internal mock)
    // Note: We don't have a direct FFI for recording relay yet, it happens inside NetworkService.
    // For this test, we'll just check if Event 9 triggers.

    println!("Waiting for economy updates...");
    std::thread::sleep(Duration::from_secs(35)); // Monitoring interval is 30s
    
    // We should have received at least one Event 9
    assert!(EVENT_9_COUNT.load(Ordering::SeqCst) >= 1);

    // 4. Attempt reward claim (should fail with "Insufficient data" or "No rewards eligible")
    println!("Attempting reward claim...");
    let res = introvert_claim_rewards_async(reward_callback);
    assert_eq!(res.code, 0);

    std::thread::sleep(Duration::from_secs(2));

    // 5. Stop Engine
    let res = introvert_engine_stop();
    assert_eq!(res.code, 0);
}
