def fix_file(main_path):
    with open(main_path, "r") as f:
        content = f.read()

    # 1. Update the call to start_dashboard_server
    target_call = """            let solana_client = match introvert::economy::solana::SolanaIncentiveEngine::new(
                "https://api.devnet.solana.com",
                "NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn",
                "https://api.introvert.network/claim",
            ) {
                Ok(c) => std::sync::Arc::new(c),
                Err(e) => {
                    error!("[Dashboard] Failed to initialize Solana client: {}", e);
                    return;
                }
            };

            if let Err(e) = start_dashboard_server(dashboard_port, operator_pubkey_str, node_name, solana_client, db_path_c).await {"""

    # Helper replacement since error vs eprintln depends on target
    is_linux = "for_linux" in main_path
    log_err = 'eprintln!("[Dashboard] Failed to initialize Solana client: {}", e);' if is_linux else 'error!("[Dashboard] Failed to initialize Solana client: {}", e);'
    log_srv_err = 'eprintln!("[Dashboard] Web GUI Server error: {}", e);' if is_linux else 'error!("[Dashboard] Web GUI Server error: {}", e);'

    replacement_call = f"""            let solana_client = match introvert::economy::solana::SolanaIncentiveEngine::new(
                "https://api.devnet.solana.com",
                "NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn",
                "https://api.introvert.network/claim",
            ) {{
                Ok(c) => std::sync::Arc::new(c),
                Err(e) => {{
                    {log_err}
                    return;
                }}
            }};

            let operator_key_bytes = signing_key.to_bytes();
            if let Err(e) = start_dashboard_server(dashboard_port, operator_pubkey_str, node_name, operator_key_bytes, solana_client, db_path_c).await {{
                {log_srv_err}
            }}"""

    content = content.replace(target_call, replacement_call)

    # 2. Update start_dashboard_server signature
    target_sig = """async fn start_dashboard_server(
    port: u16,
    operator_wallet: String,
    node_name: String,
    solana_client: std::sync::Arc<introvert::economy::solana::SolanaIncentiveEngine>,
    db_path: String,
) -> anyhow::Result<()> {"""

    replacement_sig = """async fn start_dashboard_server(
    port: u16,
    operator_wallet: String,
    node_name: String,
    operator_key_bytes: [u8; 32],
    solana_client: std::sync::Arc<introvert::economy::solana::SolanaIncentiveEngine>,
    db_path: String,
) -> anyhow::Result<()> {"""

    content = content.replace(target_sig, replacement_sig)

    # 3. Update local task spawn variables cloning
    target_clones = """        let operator_wallet = operator_wallet.clone();
        let node_name = node_name.clone();
        let solana_client = std::sync::Arc::clone(&solana_client);
        let db_path = db_path.clone();"""

    replacement_clones = """        let operator_wallet = operator_wallet.clone();
        let node_name = node_name.clone();
        let operator_key_bytes = operator_key_bytes.clone();
        let solana_client = std::sync::Arc::clone(&solana_client);
        let db_path = db_path.clone();"""

    content = content.replace(target_clones, replacement_clones)

    # 4. Update the endpoint extraction logic to use operator_key_bytes
    target_api = """            } else if request.starts_with("GET /api/export-wallet ") {
                let signing_key = match introvert::identity::NodeIdentity::derive_solana_keypair(seed_fixed_dashboard) {
                    Ok(k) => k,
                    Err(e) => {
                        eprintln!("[Dashboard] Failed to derive Solana keypair: {}", e);
                        return;
                    }
                };
                use solana_sdk::signature::Signer;
                let operator_keypair = solana_sdk::signature::Keypair::new_from_array(signing_key.to_bytes());"""

    replacement_api = """            } else if request.starts_with("GET /api/export-wallet ") {
                let signing_key = ed25519_dalek::SigningKey::from_bytes(&operator_key_bytes);
                use solana_sdk::signature::Signer;
                let operator_keypair = solana_sdk::signature::Keypair::new_from_array(signing_key.to_bytes());"""

    content = content.replace(target_api, replacement_api)

    with open(main_path, "w") as f:
        f.write(content)
    print(f"Fixed start_dashboard_server in {main_path}")

fix_file("/Users/dev/Development/introvert/src/main.rs")
fix_file("/Users/dev/Development/introvert/for_linux/src/main.rs")
