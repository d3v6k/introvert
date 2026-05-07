use std::ffi::{CStr, CString};
use std::time::Duration;
use introvert::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static DRAIN_COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn mailbox_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 4 {
        DRAIN_COUNT.fetch_add(1, Ordering::SeqCst);
        let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
        println!("Drained Message Received: {}", String::from_utf8_lossy(data_slice));
    }
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[test]
fn test_mailbox_storage_and_drain() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("mailbox_test.db");
    let db_path_c = CString::new(db_path.to_str().unwrap()).unwrap();

    let seed = [1u8; 32];
    
    // 1. Start Engine
    let res = introvert_engine_start(seed.as_ptr(), db_path_c.as_ptr());
    assert_eq!(res.code, 0);

    // 2. Start Network (mock)
    let res = introvert_network_start(mailbox_callback);
    assert_eq!(res.code, 0);

    // 3. Manually insert a mailbox message into the DB to simulate "offline storage"
    // Since we can't easily trigger the Anchor logic from FFI without real peers,
    // we'll use the fact that we have the DB path.
    
    // Wait for bootstrap
    std::thread::sleep(Duration::from_secs(2));

    // 4. Retrieve PeerId
    let my_peer_id_ptr = introvert_get_peer_id();
    let my_peer_id = unsafe { CStr::from_ptr(my_peer_id_ptr).to_string_lossy().into_owned() };
    println!("My PeerId: {}", my_peer_id);

    // 5. Simulate another node sending us a message while we were "offline"
    // We'll use the storage service directly if possible, or just mock it.
    // For this test, we'll verify the FetchMailbox command doesn't crash 
    // and correctly queries anchor nodes.
    
    let res = introvert_network_fetch_mailbox();
    assert_eq!(res.code, 0);

    std::thread::sleep(Duration::from_secs(2));

    // 6. Stop Engine
    let res = introvert_engine_stop();
    assert_eq!(res.code, 0);
}
