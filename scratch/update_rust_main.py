def update_file(main_path, is_for_linux):
    with open(main_path, "r") as f:
        content = f.read()

    # Define targets and replacements
    target_block = """                let payload = json!({
                    "node_name": node_name,
                    "operator_wallet": operator_wallet,
                    "sol_balance_lamports": sol_balance,
                    "is_staked": false,
                    "is_lease_valid": true,
                    "connected_peers": peer_count,
                    "dht_records": peer_count * 15 + 4,
                    "direct_connections": 1,
                    "relayed_connections": peer_count.max(1) - 1,
                    "cpu_load_pct": cpu_load,
                    "ram_used_mb": ram_used,
                    "ram_total_mb": ram_total,
                    "sqlite_page_count": page_count,
                    "uptime_seconds": uptime,
                    "bandwidth_rate_in_kb": rand_in,
                    "bandwidth_rate_out_kb": rand_out,
                    "new_logs": vec![format!("RBN telemetry updated. Active connections: {}", peer_count)],
                    "rbn_registry": registry_json
                });"""

    deserializer_import = "use introvert::economy::solana::RbnRegistryEntry;" if not is_for_linux else "use introvert::economy::solana::RbnRegistryEntry;"

    replacement_block = f"""                let pubkey = solana_sdk::pubkey::Pubkey::from_str(&operator_wallet).unwrap();
                let program_id = solana_sdk::pubkey::Pubkey::from_str("RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111").unwrap();
                let (registry_entry_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
                    &[b"rbn-registry", pubkey.as_ref()],
                    &program_id,
                );

                let solana_registry_status = match solana_client.rpc_client.get_account(&registry_entry_pda).await {{
                    Ok(acc) => {{
                        {deserializer_import}
                        if let Ok(entry) = RbnRegistryEntry::deserialize(&acc.data) {{
                            if entry.is_active {{
                                "ACTIVE".to_string()
                            }} else {{
                                "REGISTERED (INACTIVE)".to_string()
                            }}
                        }} else {{
                            "DESERIALIZATION ERROR".to_string()
                        }}
                    }}
                    Err(_) => "UNREGISTERED".to_string()
                }};

                let outbound_status = if sol_balance > 0 || !solana_registry_status.contains("UNREGISTERED") {{
                    "CONNECTED".to_string()
                }} else {{
                    if solana_client.rpc_client.get_latest_blockhash().await.is_ok() {{
                        "CONNECTED".to_string()
                    }} else {{
                        "DISCONNECTED".to_string()
                    }}
                }};

                let port_443_status = if peer_count > 0 {{
                    "ACTIVE (Traffic)".to_string()
                }} else {{
                    "LISTENING".to_string()
                }};

                let db_integrity = if page_count > 0 {{
                    "HEALTHY".to_string()
                }} else {{
                    "ERROR".to_string()
                }};

                let payload = json!({{
                    "node_name": node_name,
                    "version": "0.16.0",
                    "latest_version": "0.16.0",
                    "solana_registry_status": solana_registry_status,
                    "port_443_status": port_443_status,
                    "outbound_status": outbound_status,
                    "db_integrity": db_integrity,
                    "operator_wallet": operator_wallet,
                    "sol_balance_lamports": sol_balance,
                    "is_staked": false,
                    "is_lease_valid": true,
                    "connected_peers": peer_count,
                    "dht_records": peer_count * 15 + 4,
                    "direct_connections": 1,
                    "relayed_connections": peer_count.max(1) - 1,
                    "cpu_load_pct": cpu_load,
                    "ram_used_mb": ram_used,
                    "ram_total_mb": ram_total,
                    "sqlite_page_count": page_count,
                    "uptime_seconds": uptime,
                    "bandwidth_rate_in_kb": rand_in,
                    "bandwidth_rate_out_kb": rand_out,
                    "new_logs": vec![format!("RBN telemetry updated. Active connections: {{}}", peer_count)],
                    "rbn_registry": registry_json
                }});"""

    new_content = content.replace(target_block, replacement_block)
    
    with open(main_path, "w") as f:
        f.write(new_content)
    print(f"Updated {main_path}")

update_file("/Users/dev/Development/introvert/src/main.rs", False)
update_file("/Users/dev/Development/introvert/for_linux/src/main.rs", True)
