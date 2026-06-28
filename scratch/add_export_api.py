def add_api_endpoint(main_path):
    with open(main_path, "r") as f:
        content = f.read()

    target_split = """            } else if request.starts_with("GET / ") || request.starts_with("GET /index.html ") {"""

    replacement_api = """            } else if request.starts_with("GET /api/export-wallet ") {
                let signing_key = match introvert::identity::NodeIdentity::derive_solana_keypair(seed_fixed_dashboard) {
                    Ok(k) => k,
                    Err(e) => {
                        eprintln!("[Dashboard] Failed to derive Solana keypair: {}", e);
                        return;
                    }
                };
                use solana_sdk::signature::Signer;
                let operator_keypair = solana_sdk::signature::Keypair::new_from_array(signing_key.to_bytes());
                let raw_bytes = operator_keypair.to_bytes();
                let base58_priv = bs58::encode(&raw_bytes).into_string();
                let json_priv = serde_json::to_string(&raw_bytes.to_vec()).unwrap_or_default();

                let payload = json!({
                    "pubkey": operator_keypair.pubkey().to_string(),
                    "private_key_base58": base58_priv,
                    "private_key_json": json_priv
                });
                let payload_str = payload.to_string();
                format!(
                    "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                    payload_str.len(),
                    payload_str
                )
            } else if request.starts_with("GET / ") || request.starts_with("GET /index.html ") {"""

    content = content.replace(target_split, replacement_api)

    with open(main_path, "w") as f:
        f.write(content)
    print(f"Updated {main_path} with /api/export-wallet endpoint")

add_api_endpoint("/Users/dev/Development/introvert/src/main.rs")
add_api_endpoint("/Users/dev/Development/introvert/for_linux/src/main.rs")
