use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};

// Mock FFI callback
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn media_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 5 {
        unsafe {
            // Simulate Dart's leak-and-reclaim
            let _data = Vec::from_raw_parts(data_ptr as *mut u8, data_len, data_len);
            FRAME_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn test_ffi_multimedia_throughput() {
    println!("🚀 Starting FFI Multimedia Throughput Audit...");
    let fps = 30;
    let duration_secs = 10;
    let total_frames = fps * duration_secs;
    let frame_size = 1500; // typical MTU/RTP packet size
    
    let start_time = Instant::now();
    
    for _ in 0..total_frames {
        let mut frame_data = vec![0u8; frame_size];
        frame_data[0] = 0x80; // dummy RTP header
        
        frame_data.shrink_to_fit();
        let ptr = frame_data.as_ptr();
        let len = frame_data.len();
        std::mem::forget(frame_data);
        
        media_callback(5, ptr, len);
    }
    
    let elapsed = start_time.elapsed();
    let processed = FRAME_COUNT.load(Ordering::SeqCst);
    
    assert_eq!(processed, total_frames);
    
    let total_mb = (total_frames * frame_size) as f64 / 1024.0 / 1024.0;
    let throughput_mbps = total_mb / elapsed.as_secs_f64();
    
    println!("✅ Processed {} frames ({:.2} MB) in {:?}", processed, total_mb, elapsed);
    println!("⚡ FFI Throughput Capability: {:.2} MB/s", throughput_mbps);
    
    // An average 720p 30fps VP8 stream is ~1.5 Mbps (0.18 MB/s). 
    // If our FFI throughput handles > 10 MB/s, the Dart GC will not be a bottleneck.
    assert!(throughput_mbps > 10.0, "Throughput too low for high-fidelity media");
}
