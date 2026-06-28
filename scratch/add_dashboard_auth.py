def update_main_auth(main_path):
    with open(main_path, "r") as f:
        content = f.read()

    # 1. Inject helpers at the bottom of the file
    helpers_code = """
fn hash_password(password: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn get_stored_password_hash(db_path: &str) -> String {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return "c08e5e8e81561a067087093226a27e7d95393282245b73678ad9ab9bfd397e5a".to_string(), // sha256 of "introvert_rbn"
    };
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS node_config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    );
    let mut stmt = match conn.prepare("SELECT value FROM node_config WHERE key = 'dashboard_password'") {
        Ok(s) => s,
        Err(_) => return "c08e5e8e81561a067087093226a27e7d95393282245b73678ad9ab9bfd397e5a".to_string(),
    };
    let hash_opt: Result<String, _> = stmt.query_row([], |row| row.get(0));
    hash_opt.unwrap_or_else(|_| {
        // Default to SHA256 hash of "introvert_rbn"
        "c08e5e8e81561a067087093226a27e7d95393282245b73678ad9ab9bfd397e5a".to_string()
    })
}

fn set_stored_password_hash(db_path: &str, hash: &str) -> anyhow::Result<()> {
    let conn = rusqlite::Connection::open(db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS node_config (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO node_config (key, value) VALUES ('dashboard_password', ?1)",
        [hash],
    )?;
    Ok(())
}
"""
    content += helpers_code

    # 2. Add session_token variable initialization and cloning in start_dashboard_server
    target_start_srv = """    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("[Dashboard] Web GUI Server listening on http://0.0.0.0:{}", port);

    let start_time = tokio::time::Instant::now();

    loop {
        let (mut socket, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let operator_wallet = operator_wallet.clone();
        let node_name = node_name.clone();
        let operator_key_bytes = operator_key_bytes.clone();
        let solana_client = std::sync::Arc::clone(&solana_client);
        let db_path = db_path.clone();"""

    replacement_start_srv = """    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("[Dashboard] Web GUI Server listening on http://0.0.0.0:{}", port);

    let start_time = tokio::time::Instant::now();
    let session_token = std::sync::Arc::new(parking_lot::Mutex::new(None::<String>));

    loop {
        let (mut socket, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let operator_wallet = operator_wallet.clone();
        let node_name = node_name.clone();
        let operator_key_bytes = operator_key_bytes.clone();
        let solana_client = std::sync::Arc::clone(&solana_client);
        let db_path = db_path.clone();
        let session_token = std::sync::Arc::clone(&session_token);"""

    content = content.replace(target_start_srv, replacement_start_srv)

    # 3. Add token extraction logic in spawn task
    target_spawn_task = """            let request = String::from_utf8_lossy(&buf[..n]);
            let response = if request.starts_with("GET /api/stats ") {"""

    replacement_spawn_task = """            let request = String::from_utf8_lossy(&buf[..n]);
            let request_line = request.lines().next().unwrap_or("");
            let has_valid_token = {
                let token_guard = session_token.lock();
                if let Some(ref active_token) = *token_guard {
                    request_line.contains(&format!("token={}", active_token))
                } else {
                    false
                }
            };

            let response = if request_line.starts_with("GET /api/stats") {
                if !has_valid_token {
                    let payload = json!({"status": "unauthorized"});
                    let payload_str = payload.to_string();
                    format!(
                        "HTTP/1.1 401 Unauthorized\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                        payload_str.len(),
                        payload_str
                    )
                } else {"""

    content = content.replace(target_spawn_task, replacement_spawn_task)

    # 4. Modify stats branch end and add export-wallet branch protection + login + change-password routes
    target_export_wallet = """                let payload_str = payload.to_string();
                format!(
                    "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                    payload_str.len(),
                    payload_str
                )
            } else if request.starts_with("GET /api/export-wallet ") {
                let signing_key = ed25519_dalek::SigningKey::from_bytes(&operator_key_bytes);
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

    replacement_export_wallet = """                let payload_str = payload.to_string();
                format!(
                    "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                    payload_str.len(),
                    payload_str
                )
                } // End of stats authenticated branch
            } else if request_line.starts_with("GET /api/export-wallet") {
                if !has_valid_token {
                    let payload = json!({"status": "unauthorized"});
                    let payload_str = payload.to_string();
                    format!(
                        "HTTP/1.1 401 Unauthorized\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                        payload_str.len(),
                        payload_str
                    )
                } else {
                    let signing_key = ed25519_dalek::SigningKey::from_bytes(&operator_key_bytes);
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
                }
            } else if request_line.starts_with("GET /api/login") {
                let password = request_line
                    .split("password=")
                    .nth(1)
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.split('&').next())
                    .unwrap_or("");
                
                let expected_hash = get_stored_password_hash(&db_path);
                let input_hash = hash_password(password);

                if input_hash == expected_hash {
                    let rand_bytes: [u8; 16] = rand::random();
                    let new_token = hex::encode(rand_bytes);
                    *session_token.lock() = Some(new_token.clone());

                    let payload = json!({
                        "status": "success",
                        "token": new_token
                    });
                    let payload_str = payload.to_string();
                    format!(
                        "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                        payload_str.len(),
                        payload_str
                    )
                } else {
                    let payload = json!({
                        "status": "failed",
                        "message": "Invalid password"
                    });
                    let payload_str = payload.to_string();
                    format!(
                        "HTTP/1.1 401 Unauthorized\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                        payload_str.len(),
                        payload_str
                    )
                }
            } else if request_line.starts_with("GET /api/change-password") {
                if !has_valid_token {
                    let payload = json!({"status": "unauthorized"});
                    let payload_str = payload.to_string();
                    format!(
                        "HTTP/1.1 401 Unauthorized\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                        payload_str.len(),
                        payload_str
                    )
                } else {
                    let old_pwd = request_line
                        .split("old=")
                        .nth(1)
                        .and_then(|s| s.split(' ').next())
                        .and_then(|s| s.split('&').next())
                        .unwrap_or("");
                    let new_pwd = request_line
                        .split("new=")
                        .nth(1)
                        .and_then(|s| s.split(' ').next())
                        .and_then(|s| s.split('&').next())
                        .unwrap_or("");

                    let expected_hash = get_stored_password_hash(&db_path);
                    let input_hash = hash_password(old_pwd);

                    if input_hash == expected_hash {
                        let new_hash = hash_password(new_pwd);
                        let payload = match set_stored_password_hash(&db_path, &new_hash) {
                            Ok(_) => json!({"status": "success"}),
                            Err(_) => json!({"status": "failed", "message": "Failed to update db"}),
                        };
                        let payload_str = payload.to_string();
                        format!(
                            "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                            payload_str.len(),
                            payload_str
                        )
                    } else {
                        let payload = json!({"status": "failed", "message": "Incorrect current password"});
                        let payload_str = payload.to_string();
                        format!(
                            "HTTP/1.1 400 Bad Request\\r\\nContent-Type: application/json\\r\\nAccess-Control-Allow-Origin: *\\r\\nContent-Length: {}\\r\\n\\r\\n{}",
                            payload_str.len(),
                            payload_str
                        )
                    }
                }
            } else if request_line.starts_with("GET / ") || request_line.starts_with("GET /index.html ") {"""

    content = content.replace(target_export_wallet, replacement_export_wallet)

    with open(main_path, "w") as f:
        f.write(content)
    print(f"Auth system successfully integrated in {main_path}")

update_main_auth("/Users/dev/Development/introvert/src/main.rs")
update_main_auth("/Users/dev/Development/introvert/for_linux/src/main.rs")
