use std::ffi::{CString, CStr};
use tempfile::tempdir;
use introvert::{introvert_engine_start, introvert_get_peer_id, introvert_store_message_async, introvert_engine_stop, introvert_free_string, FfiResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

extern "C" fn test_callback(res: FfiResult) {
    assert_eq!(res.code, 0);
    CALLBACK_FIRED.store(true, Ordering::SeqCst);
}

static CALLBACK_FIRED: AtomicBool = AtomicBool::new(false);

#[test]
fn test_unbreakable_foundation_async() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("introvert_test.db");
    let db_path_c = CString::new(db_path.to_str().unwrap()).unwrap();

    let seed = [7u8; 32];

    // 1. Start Engine
    let start_res = introvert_engine_start(seed.as_ptr(), db_path_c.as_ptr());
    assert_eq!(start_res.code, 0, "Engine failed to start");

    // 2. Fetch and verify PeerId
    let peer_id_ptr = introvert_get_peer_id();
    assert!(!peer_id_ptr.is_null());
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    assert!(!peer_id.is_empty());
    introvert_free_string(peer_id_ptr);

    // 3. Store a test message asynchronously
    let test_peer = CString::new("test_peer_123").unwrap();
    let test_msg = CString::new("Hello from the ground up!").unwrap();
    
    CALLBACK_FIRED.store(false, Ordering::SeqCst);
    let store_res = introvert_store_message_async(test_peer.as_ptr(), test_msg.as_ptr(), false, test_callback);
    assert_eq!(store_res.code, 0, "Failed to submit message task");

    // Wait for callback (simple spin wait for test)
    let mut timeout = 100; // 10 seconds
    while !CALLBACK_FIRED.load(Ordering::SeqCst) && timeout > 0 {
        std::thread::sleep(Duration::from_millis(100));
        timeout -= 1;
    }
    assert!(CALLBACK_FIRED.load(Ordering::SeqCst), "Callback timed out");

    // 4. Stop Engine
    let stop_res = introvert_engine_stop();
    assert_eq!(stop_res.code, 0, "Failed to stop engine");
}
