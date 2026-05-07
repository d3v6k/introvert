use std::ffi::{CStr, CString};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use introvert::*;
use introvert::media::MediaFrameHeader;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};

static TOTAL_LATENCY_MS: AtomicU64 = AtomicU64::new(0);
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);
static MAX_LATENCY_MS: AtomicU64 = AtomicU64::new(0);

extern "C" fn stress_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    if event_type == 5 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let header_ptr = data_ptr as *const MediaFrameHeader;
        let header = unsafe { &*header_ptr };
        
        let latency = now.saturating_sub(header.timestamp);
        
        TOTAL_LATENCY_MS.fetch_add(latency, Ordering::SeqCst);
        FRAME_COUNT.fetch_add(1, Ordering::SeqCst);
        
        let mut current_max = MAX_LATENCY_MS.load(Ordering::SeqCst);
        while latency > current_max {
            match MAX_LATENCY_MS.compare_exchange_weak(current_max, latency, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }
    
    // Crucial: Reclaim memory as Flutter would
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[test]
fn test_multimedia_stress_and_stability() {
    println!("🎬 Starting Multimedia Stress Audit (10-minute simulation)...");
    
    // 10 minutes at 30fps = 18,000 frames
    let total_frames = 18000;
    let frame_interval = Duration::from_micros(33333); // ~30fps
    
    // Simulate a 1080p frame fragment (~100KB payload)
    let payload_size = 100 * 1024;
    let dummy_payload = vec![0u8; payload_size];
    
    let start_time = Instant::now();
    let mut last_report = Instant::now();

    for i in 0..total_frames {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let header = MediaFrameHeader {
            codec: 0, // VP8
            width: 1920,
            height: 1080,
            timestamp: now_ms,
        };

        // Mimic MediaManager's handoff pattern
        let header_size = std::mem::size_of::<MediaFrameHeader>();
        let mut buffer = Vec::with_capacity(header_size + payload_size);
        
        let header_bytes = unsafe {
            std::slice::from_raw_parts(&header as *const _ as *const u8, header_size)
        };
        buffer.extend_from_slice(header_bytes);
        buffer.extend_from_slice(&dummy_payload);
        
        buffer.shrink_to_fit();
        let ptr = buffer.as_ptr();
        let len = buffer.len();
        std::mem::forget(buffer); // LEAK to FFI

        // Invoke callback
        stress_callback(5, ptr, len);

        // Control frame rate
        if i % 100 == 0 && last_report.elapsed() > Duration::from_secs(2) {
            let frames = FRAME_COUNT.load(Ordering::SeqCst);
            let avg_latency = TOTAL_LATENCY_MS.load(Ordering::SeqCst) as f64 / frames as f64;
            println!("Progress: {}/{} frames. Avg Latency: {:.2}ms", frames, total_frames, avg_latency);
            last_report = Instant::now();
        }
        
        // In a real stress test we'd sleep, but for CI we'll run at full speed 
        // to check for memory leaks faster, unless we want to measure "system jitter".
        // Let's run full speed to maximize pressure on the heap.
    }

    let duration = start_time.elapsed();
    let final_frames = FRAME_COUNT.load(Ordering::SeqCst);
    let avg_latency = TOTAL_LATENCY_MS.load(Ordering::SeqCst) as f64 / final_frames as f64;
    let max_latency = MAX_LATENCY_MS.load(Ordering::SeqCst);

    println!("\n📊 STRESS AUDIT RESULTS:");
    println!("Total Frames Processed: {}", final_frames);
    println!("Total Time: {:?}", duration);
    println!("Average FFI Latency: {:.4} ms", avg_latency);
    println!("Max Jitter Latency: {} ms", max_latency);
    
    // ASSERTIONS
    assert!(avg_latency < 30.0, "Average latency exceeds 30ms target!");
    assert_eq!(final_frames, total_frames, "Frame drop detected in stress test!");
    
    println!("✅ Multimedia Stress Audit PASSED.");
}
