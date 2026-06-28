use std::ffi::{CStr, CString};
use std::time::Duration;
use introvert::*; // Import FfiResult and other types from lib.rs

// Mock Network Callback
extern "C" fn mock_network_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
    let data_str = String::from_utf8_lossy(data_slice);
    println!("Received network event: {} with data: {}", event_type, data_str);
    
    // Memory Check: Verify "Leaked Heap" pattern by reclaiming memory
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[test]
fn test_ffi_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_introvert.db");
    let db_path_str = db_path.to_str().unwrap();
    let db_path_c = CString::new(db_path_str).unwrap();

    // 1. Generate Mnemonic and Seed
    let mnemonic_ptr = introvert_generate_mnemonic();
    assert!(!mnemonic_ptr.is_null());
    
    let seed_res = introvert_mnemonic_to_seed(mnemonic_ptr);
    assert_eq!(seed_res.code, 0);
    assert!(!seed_res.data.is_null());
    assert_eq!(seed_res.len, 32);

    let mut seed = [0u8; 32];
    unsafe { std::ptr::copy_nonoverlapping(seed_res.data, seed.as_mut_ptr(), 32); }
    introvert_free_binary(seed_res.data, seed_res.len);
    
    // Free mnemonic string
    introvert_free_string(mnemonic_ptr);

    // 2. Start Engine
    let res = introvert_engine_start(seed.as_ptr(), db_path_c.as_ptr());
    assert_eq!(res.code, 0);

    // 3. Start Network
    let res = introvert_network_start(mock_network_callback);
    assert_eq!(res.code, 0);

    // 4. Add manual address (bootstrap)
    let peer_id_str = "12D3KooWSWfVJhYkmFqQPS4B2K8agJ5f1PqV2aMPMh1F1fLg9Ahb"; 
    let addr_str = "/ip4/127.0.0.1/tcp/4001";
    let peer_id_c = CString::new(peer_id_str).unwrap();
    let addr_c = CString::new(addr_str).unwrap();
    let res = introvert_network_add_address(peer_id_c.as_ptr(), addr_c.as_ptr());
    assert_eq!(res.code, 0);

    // Small delay to allow network to process
    std::thread::sleep(Duration::from_millis(1000));

    // 5. Get Peer ID
    let peer_id_ptr = introvert_get_peer_id();
    assert!(!peer_id_ptr.is_null());
    let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy().into_owned() };
    println!("My Peer ID: {}", peer_id);
    introvert_free_string(peer_id_ptr);

    // Test RBN FFI functions
    let rbn_res = introvert_network_get_rbns();
    assert_eq!(rbn_res.code, 0);
    assert!(!rbn_res.data.is_null());
    let rbn_json = unsafe { std::slice::from_raw_parts(rbn_res.data, rbn_res.len) };
    let rbn_str = std::str::from_utf8(rbn_json).unwrap();
    println!("RBN List from FFI: {}", rbn_str);
    introvert_free_binary(rbn_res.data, rbn_res.len);

    let test_ip_c = CString::new("127.0.0.1").unwrap();
    let test_rbn_res = introvert_network_test_rbn(test_ip_c.as_ptr());
    assert_eq!(test_rbn_res.code, 0);

    // 6. Stop Engine
    let res = introvert_engine_stop();
    assert_eq!(res.code, 0);
}
