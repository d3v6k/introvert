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
    #[arg(short, long)]
    seed_file: Option<PathBuf>,

    /// Path to the SQLCipher database file
    #[arg(short, long, default_value = "introvert.db")]
    db_path: String,

    /// TCP port to listen on
    #[arg(short, long, default_value_t = 443)]
    port: u16,

    /// Enable global relay server functionality
    #[arg(short, long, default_value_t = false)]
    relay: bool,

    /// Legacy support: Path to the data directory
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Maximum number of concurrent connections (Production Scale)
    #[arg(long, default_value_t = 1000000)]
    max_connections: u32,

    /// Interval for K-Bucket liveness checks in seconds
    #[arg(long, default_value_t = 300)]
    liveness_check: u64,

    /// Port to run the WebSocket tunnel on
    #[arg(long, default_value_t = 80)]
    tunnel_port: u16,
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
            let status = data_slice.first().cloned().unwrap_or(2);
            let status_text = match status {
                0 => "DIRECT",
                1 => "RELAYED",
                _ => "OFFLINE",
            };
            println!("[Network] Connection Status Change: {}", status_text);
        }
        10 => {
            let status = data_slice.first().cloned().unwrap_or(0);
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
    if let Some(dir) = args.data_dir.clone() {
        if args.db_path == "introvert.db" {
            args.db_path = dir.join("introvert.db").to_string_lossy().into_owned();
        }
        if args.seed_file.is_none() {
            let default_seed = dir.join("introvert.seed");
            if default_seed.exists() {
                args.seed_file = Some(default_seed);
            }
        }
    }

    // 1. Load Seed
    let seed: Vec<u8> = if let Ok(env_seed) = std::env::var("INTROVERT_SEED") {
        println!("[System] Loading master seed from INTROVERT_SEED environment variable.");
        hex::decode(env_seed.trim()).map_err(|e| anyhow::anyhow!("Invalid hex in INTROVERT_SEED: {}", e))?
    } else if let Some(path) = &args.seed_file {
        if !path.exists() {
            anyhow::bail!("Seed file not found at {:?}", path);
        }
        fs::read(path)?
    } else {
        // Fallback to prompt
        println!("[Security] No master seed provided via environment or file.");
        let input = rpassword::prompt_password("Enter Master Seed (Hex, 32 bytes): ")?;
        hex::decode(input.trim()).map_err(|e| anyhow::anyhow!("Invalid hex input: {}", e))?
    };

    if seed.len() != 32 {
        anyhow::bail!("Error: Master seed must be exactly 32 bytes (64 hex characters). Found {} bytes.", seed.len());
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

    // 3. Start WebSocket Tunnel Server if running as a relay/RBN
    if args.relay {
        println!("[System] Starting WebSocket tunnel server on port {}...", args.tunnel_port);
        let libp2p_port = args.port;
        let tunnel_port = args.tunnel_port;
        tokio::spawn(async move {
            if let Err(e) = introvert::network::tunnel::start_tunnel_server(tunnel_port, libp2p_port).await {
                eprintln!("WebSocket tunnel server failed to start: {}", e);
            }
        });
    }

    // 4. Start Network
    let res = introvert_network_start_production(
        daemon_network_callback, 
        args.port, 
        args.relay,
        args.max_connections,
        args.liveness_check,
    );
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

    // 5. Handle Shutdown
    signal::ctrl_c().await?;
    println!("\nShutting down...");

    let res = introvert_engine_stop();
    if res.code != 0 {
        let msg = unsafe { CStr::from_ptr(res.data as *mut c_char).to_string_lossy() };
        eprintln!("Error during shutdown ({}): {}", res.code, msg);
    }

    Ok(())
}
