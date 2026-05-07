use clap::Parser;
use std::ffi::{CString, CStr, c_char};
use std::path::PathBuf;
use std::fs;
use tokio::signal;
use introvert::*; // Import FfiResult and engine controls

#[derive(Parser, Debug)]
#[command(author, version, about = "Introvert Headless Daemon", long_about = None)]
struct Args {
    /// Path to the 32-byte master seed file (binary)
    #[arg(short, long, default_value = "introvert.seed")]
    seed_file: PathBuf,

    /// Path to the SQLCipher database file
    #[arg(short, long, default_value = "introvert.db")]
    db_path: String,

    /// TCP port to listen on
    #[arg(short, long, default_value_t = 4001)]
    port: u16,

    /// Enable global relay server functionality
    #[arg(short, long, default_value_t = false)]
    relay: bool,

    /// Legacy support: Path to the data directory
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

// Global callback for the headless daemon
extern "C" fn daemon_network_callback(event_type: i32, data_ptr: *const u8, data_len: usize) {
    let data_slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };
    
    match event_type {
        1 => {
            // PeerID is now binary
            let hex_id = hex::encode(data_slice);
            println!("[Network] Peer Discovered (Binary/Hex): {}", hex_id);
        }
        2 => {
            let data_str = String::from_utf8_lossy(data_slice);
            println!("[Network] Signaling Message: {}", data_str);
        }
        3 => println!("[Network] WebRTC Channel Open"),
        4 => println!("[Network] WebRTC Data Received ({} bytes)", data_len),
        8 => {
            let status = data_slice.get(0).cloned().unwrap_or(2);
            let status_text = match status {
                0 => "DIRECT",
                1 => "RELAYED",
                _ => "OFFLINE",
            };
            println!("[Network] Connection Status Change: {}", status_text);
        }
        10 => {
            let status = data_slice.get(0).cloned().unwrap_or(0);
            let status_text = match status {
                1 => "ONLINE (Listening)",
                2 => "RELAY CONNECTED",
                _ => "OFFLINE",
            };
            println!("[Network] Local Node Status: {}", status_text);
        }
        _ => {
            let data_str = String::from_utf8_lossy(data_slice);
            println!("[Network] Unknown Event {}: {}", event_type, data_str);
        }
    }

    // Memory Reclamation
    introvert_free_binary(data_ptr as *mut u8, data_len);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();

    // Handle --data-dir legacy support
    if let Some(dir) = args.data_dir {
        // If data_dir is provided, we expect the seed and db to be inside it 
        // unless they were explicitly overridden by -s or -d
        if args.db_path == "introvert.db" {
            args.db_path = dir.join("introvert.db").to_string_lossy().into_owned();
        }
        if args.seed_file == PathBuf::from("introvert.seed") {
            args.seed_file = dir.join("introvert.seed");
        }
    }

    // 1. Load Seed
    if !args.seed_file.exists() {
        eprintln!("Error: Seed file not found at {:?}", args.seed_file);
        println!("Hint: Create a 32-byte binary file containing your master seed.");
        std::process::exit(1);
    }

    let seed = fs::read(&args.seed_file)?;
    if seed.len() != 32 {
        eprintln!("Error: Seed file must be exactly 32 bytes (found {}).", seed.len());
        std::process::exit(1);
    }
    
    let mut seed_fixed = [0u8; 32];
    seed_fixed.copy_from_slice(&seed);

    // 2. Initialize Engine
    let db_path_c = CString::new(args.db_path.clone())?;
    let res = introvert_engine_start(seed_fixed.as_ptr(), db_path_c.as_ptr());
    if res.code != 0 {
        let msg = unsafe { CStr::from_ptr(res.data as *mut c_char).to_string_lossy() };
        eprintln!("Failed to start engine ({}): {}", res.code, msg);
        std::process::exit(1);
    }

    println!("Introvert Engine started successfully.");

    // 3. Start Network
    let res = introvert_network_start_ext(daemon_network_callback, args.port, args.relay);
    if res.code != 0 {
        let msg = unsafe { CStr::from_ptr(res.data as *mut c_char).to_string_lossy() };
        eprintln!("Failed to start network ({}): {}", res.code, msg);
        std::process::exit(1);
    }

    let peer_id_ptr = introvert_get_peer_id();
    if !peer_id_ptr.is_null() {
        let peer_id = unsafe { CStr::from_ptr(peer_id_ptr).to_string_lossy() };
        println!("Headless Node Online.");
        println!("PEER_ID={}", peer_id);
        introvert_free_string(peer_id_ptr);
    }

    println!("Press Ctrl+C to stop...");

    // 4. Handle Shutdown
    signal::ctrl_c().await?;
    println!("\nShutting down...");

    let res = introvert_engine_stop();
    if res.code != 0 {
        let msg = unsafe { CStr::from_ptr(res.data as *mut c_char).to_string_lossy() };
        eprintln!("Error during shutdown ({}): {}", res.code, msg);
    }

    Ok(())
}
