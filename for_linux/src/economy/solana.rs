use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
    pubkey::Pubkey,
    instruction::Instruction,
    message::Message,
};
use anyhow::{Result, anyhow};
use std::str::FromStr;
use serde_json::json;
use base64::{Engine as _, engine::general_purpose};
use std::sync::Arc;

pub struct SolanaIncentiveEngine {
    pub rpc_client: Arc<RpcClient>,
    http_client: reqwest::Client,
    intr_mint: Pubkey,
    treasury_pubkey: Pubkey,
    treasury_api_url: String, // Endpoint for fee-payer co-signing
}

impl SolanaIncentiveEngine {
    pub fn new(rpc_url: &str, treasury_pubkey: &str, treasury_api_url: &str) -> Result<Self> {
        Ok(Self {
            rpc_client: Arc::new(RpcClient::new_with_timeout(rpc_url.to_string(), std::time::Duration::from_secs(3))),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            intr_mint: Pubkey::from_str("EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf")?,
            treasury_pubkey: Pubkey::from_str(treasury_pubkey)?,
            treasury_api_url: treasury_api_url.to_string(),
        })
    }

    pub fn get_treasury_pubkey(&self) -> Pubkey {
        self.treasury_pubkey
    }

    /// Fetches the INTR token balance for a given owner.
    /// Uses lightweight getAccountInfo with known ATA address instead of heavy getProgramAccounts.
    pub async fn fetch_balance(&self, owner: &Pubkey) -> Result<u64> {
        // Derive the Associated Token Account (ATA) address
        // ATA = find_program_address(&[owner, TOKEN_PROGRAM_ID, mint], ASSOCIATED_TOKEN_PROGRAM_ID)
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")?;
        let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")?;
        
        let (ata, _) = Pubkey::find_program_address(
            &[&owner.to_bytes(), &token_program.to_bytes(), &self.intr_mint.to_bytes()],
            &ata_program,
        );
        
        match self.rpc_client.get_account(&ata).await {
            Ok(account) => {
                // SPL Token account: amount is at bytes 64..72 (little-endian u64)
                if account.data.len() >= 72 {
                    let mut amount_bytes = [0u8; 8];
                    amount_bytes.copy_from_slice(&account.data[64..72]);
                    Ok(u64::from_le_bytes(amount_bytes))
                } else {
                    Ok(0)
                }
            }
            Err(_) => Ok(0), // Account doesn't exist yet (no tokens)
        }
    }

    /// Fetches the native SOL balance for a given owner.
    pub async fn fetch_sol_balance(&self, owner: &Pubkey) -> Result<u64> {
        let lamports = self.rpc_client.get_balance(owner).await?;
        Ok(lamports)
    }

    /// Fetches the balance of an SPL token (by mint) for a given owner.
    pub async fn fetch_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        use solana_account_decoder::UiAccountEncoding;
        use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
        use solana_client::rpc_filter::{Memcmp, RpcFilterType};

        let filters = Some(vec![
            RpcFilterType::DataSize(165),
            RpcFilterType::Memcmp(Memcmp::new(
                32,
                solana_client::rpc_filter::MemcmpEncodedBytes::Base58(owner.to_string()),
            )),
            RpcFilterType::Memcmp(Memcmp::new(
                0,
                solana_client::rpc_filter::MemcmpEncodedBytes::Base58(mint.to_string()),
            )),
        ]);

        let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")?;

        let accounts = self.rpc_client.get_program_ui_accounts_with_config(
            &token_program_id,
            RpcProgramAccountsConfig {
                filters,
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        ).await?;

        if let Some((_, account)) = accounts.first() {
            match &account.data {
                solana_account_decoder::UiAccountData::Binary(data, _) => {
                    if let Ok(decoded) = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD, data
                    ) {
                        if decoded.len() >= 72 {
                            let mut amount_bytes = [0u8; 8];
                            amount_bytes.copy_from_slice(&decoded[64..72]);
                            return Ok(u64::from_le_bytes(amount_bytes));
                        }
                    }
                    Ok(0)
                }
                _ => Ok(0),
            }
        } else {
            Ok(0)
        }
    }

    /// Submits a reward claim proof to the Solana network using a gasless fee-payer model.
    pub async fn submit_reward_claim(&self, user_keypair: &Keypair, proof: &[u8]) -> Result<String> {
        // 1. Create a placeholder instruction for the reward (Memo Program)
        let memo_program_id = Pubkey::from_str("MemoSq4gqABAXDe96DnMs8JmJ6swv6Yy6pEqiaMoL64")?;
        
        let instruction = Instruction::new_with_bytes(
            memo_program_id,
            proof,
            vec![], 
        );

        // 2. Fetch recent blockhash asynchronously
        let blockhash = self.rpc_client.get_latest_blockhash().await?;

        // 3. Construct the Message with Treasury as the Fee Payer
        let message = Message::new_with_blockhash(
            &[instruction],
            Some(&self.treasury_pubkey),
            &blockhash,
        );

        // 4. Create a Transaction and sign it with the user's keypair
        let mut tx = Transaction::new_unsigned(message);
        tx.partial_sign(&[user_keypair], blockhash);

        // 5. Serialize and Encode
        let serialized_tx = bincode::serialize(&tx)?;
        let base64_tx = general_purpose::STANDARD.encode(serialized_tx);

        // 6. Relay to Treasury API for co-signing and submission
        self.relay_to_treasury(base64_tx).await
    }

    async fn relay_to_treasury(&self, base64_tx: String) -> Result<String> {
        let payload = json!({
            "transaction": base64_tx,
        });

        let mut request = self.http_client.post(&self.treasury_api_url)
            .json(&payload);

        // Attach auth token if configured (closes anonymous endpoint exposure)
        if let Ok(auth_token) = std::env::var("INTROVERT_TREASURY_AUTH") {
            if !auth_token.is_empty() {
                request = request.header("X-Introvert-Auth", auth_token);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| anyhow!("Relay request failed: {}", e))?;

        if response.status().is_success() {
            let res_body: serde_json::Value = response.json().await?;
            let signature = res_body["signature"]
                .as_str()
                .ok_or_else(|| anyhow!("No signature in response"))?;
            Ok(signature.to_string())
        } else {
            let err_text = response.text().await?;
            Err(anyhow!("Treasury relay error: {}", err_text))
        }
    }

    /// Fetches all registered RBN nodes from the on-chain Solana registry.
    pub async fn fetch_registered_rbns(&self, program_id_str: &str) -> Result<Vec<(String, String)>> {
        use solana_client::rpc_config::RpcProgramAccountsConfig;
        use solana_client::rpc_filter::RpcFilterType;
        use solana_account_decoder::{UiAccountEncoding, UiAccountData};

        let program_id = Pubkey::from_str(program_id_str)?;

        let config = RpcProgramAccountsConfig {
            filters: None,
            account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..Default::default()
            },
            ..Default::default()
        };

        let accounts = self.rpc_client.get_program_ui_accounts_with_config(&program_id, config).await?;
        let mut rbn_nodes = Vec::new();

        for (_pubkey, account) in accounts {
            match &account.data {
                UiAccountData::Binary(base64_str, _) => {
                    if let Ok(decoded_data) = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        base64_str
                    ) {
                        if let Ok(entry) = RbnRegistryEntry::deserialize(&decoded_data) {
                            if entry.is_active {
                                rbn_nodes.push((entry.peer_id, entry.multiaddresses));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(rbn_nodes)
    }

    /// Registers or updates the RBN node registration details on-chain.
    pub async fn register_rbn_on_chain(
        &self,
        operator_keypair: &Keypair,
        peer_id: &str,
        multiaddresses: &str,
        node_name: &str,
        program_id_str: &str,
    ) -> Result<String> {
        let program_id = Pubkey::from_str(program_id_str)?;
        let operator_pubkey = operator_keypair.pubkey();
        
        let (registry_entry_pda, _) = Pubkey::find_program_address(
            &[b"rbn-registry", operator_pubkey.as_ref()],
            &program_id,
        );

        let system_program = Pubkey::from_str("11111111111111111111111111111111")?;

        // 1. Check if registry account already exists
        let mut account_exists = false;
        let mut needs_update = true;

        if let Ok(account) = self.rpc_client.get_account(&registry_entry_pda).await {
            account_exists = true;
            if let Ok(entry) = RbnRegistryEntry::deserialize(&account.data) {
                if entry.peer_id == peer_id && entry.multiaddresses == multiaddresses && entry.node_name == node_name && entry.is_active {
                    needs_update = false;
                    tracing::info!("[SolanaRegistry] Node is already registered on-chain with identical details. Skipping registration.");
                }
            }
        }

        if !needs_update {
            return Ok("Skipped (Already Registered)".to_string());
        }

        // 2. Build instruction
        let (instruction, tx_type) = if !account_exists {
            tracing::info!("[SolanaRegistry] Initializing RBN registration on-chain...");
            let discriminator = anchor_discriminator("global", "register_rbn");
            let mut data = Vec::new();
            data.extend_from_slice(&discriminator);
            data.extend_from_slice(&borsh_serialize_string(peer_id));
            data.extend_from_slice(&borsh_serialize_string(multiaddresses));
            data.extend_from_slice(&borsh_serialize_string(node_name));

            let accounts = vec![
                solana_sdk::instruction::AccountMeta::new(registry_entry_pda, false),
                solana_sdk::instruction::AccountMeta::new(operator_pubkey, true),
                solana_sdk::instruction::AccountMeta::new_readonly(system_program, false),
            ];
            
            (Instruction::new_with_bytes(program_id, &data, accounts), "RegisterRbn")
        } else {
            tracing::info!("[SolanaRegistry] Updating RBN registration on-chain...");
            let discriminator = anchor_discriminator("global", "update_rbn");
            let mut data = Vec::new();
            data.extend_from_slice(&discriminator);
            data.extend_from_slice(&borsh_serialize_string(peer_id));
            data.extend_from_slice(&borsh_serialize_string(multiaddresses));
            data.extend_from_slice(&borsh_serialize_string(node_name));
            data.push(1); // is_active = true

            let accounts = vec![
                solana_sdk::instruction::AccountMeta::new(registry_entry_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(operator_pubkey, true),
            ];

            (Instruction::new_with_bytes(program_id, &data, accounts), "UpdateRbn")
        };

        // 3. Create Transaction
        let blockhash = self.rpc_client.get_latest_blockhash().await?;
        let message = Message::new_with_blockhash(
            &[instruction],
            Some(&operator_pubkey),
            &blockhash,
        );

        let mut tx = Transaction::new_unsigned(message);
        tx.sign(&[operator_keypair], blockhash);

        // 4. Send and Confirm using direct raw JSON-RPC to bypass trait version mismatches
        tracing::info!("[SolanaRegistry] Submitting {} transaction to devnet...", tx_type);
        let signature = self.send_transaction_raw(&tx).await?;
        tracing::info!("[SolanaRegistry] Transaction successful! Signature: {}", signature);
        
        Ok(signature.to_string())
    }

    /// Fetches full registration details for all active RBNs from the Solana program.
    pub async fn fetch_registered_rbn_details(
        &self,
        program_id_str: &str,
    ) -> Result<Vec<RbnRegistryEntry>> {
        use solana_client::rpc_config::RpcProgramAccountsConfig;
        use solana_account_decoder::{UiAccountEncoding, UiAccountData};

        let program_id = Pubkey::from_str(program_id_str)?;
        let config = RpcProgramAccountsConfig {
            filters: None,
            account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..Default::default()
            },
            ..Default::default()
        };

        let accounts = self.rpc_client.get_program_ui_accounts_with_config(&program_id, config).await?;
        let mut rbn_nodes = Vec::new();

        for (_pubkey, account) in accounts {
            match &account.data {
                UiAccountData::Binary(base64_str, _) => {
                    if let Ok(decoded_data) = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        base64_str
                    ) {
                        if let Ok(entry) = RbnRegistryEntry::deserialize(&decoded_data) {
                            rbn_nodes.push(entry);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(rbn_nodes)
    }

    /// Calls `update_rbn_routing` on the Introvert Handle Registry Anchor program.
    /// Updates the `ip_address` field for a handle entry on-chain.
    pub async fn update_rbn_routing(
        &self,
        owner_keypair: &Keypair,
        handle: &str,
        new_ip_address: &str,
    ) -> Result<String> {
        let program_id = Pubkey::from_str("FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW")?;
        let owner_pubkey = owner_keypair.pubkey();
        let system_program = Pubkey::from_str("11111111111111111111111111111111")?;

        // Derive the handle entry PDA: seeds = [b"handle", handle.as_bytes()]
        let (handle_entry_pda, _) = Pubkey::find_program_address(
            &[b"handle", handle.as_bytes()],
            &program_id,
        );

        // Build instruction data: discriminator + borsh-serialized new_ip_address
        let discriminator = anchor_discriminator("global", "update_rbn_routing");
        let mut data = Vec::new();
        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&borsh_serialize_string(new_ip_address));

        let accounts = vec![
            solana_sdk::instruction::AccountMeta::new(handle_entry_pda, false),
            solana_sdk::instruction::AccountMeta::new(owner_pubkey, true),
            solana_sdk::instruction::AccountMeta::new_readonly(system_program, false),
        ];

        let instruction = Instruction::new_with_bytes(program_id, &data, accounts);

        let blockhash = self.rpc_client.get_latest_blockhash().await?;
        let message = Message::new_with_blockhash(
            &[instruction],
            Some(&owner_pubkey),
            &blockhash,
        );

        let mut tx = Transaction::new_unsigned(message);
        tx.sign(&[owner_keypair], blockhash);

        tracing::info!("[RBN Routing] Submitting update_rbn_routing for handle '{}' with ip_address='{}'", handle, new_ip_address);
        let signature = self.send_transaction_raw(&tx).await?;
        tracing::info!("[RBN Routing] Transaction successful! Signature: {}", signature);

        Ok(signature)
    }

    async fn send_transaction_raw(&self, tx: &Transaction) -> Result<String> {
        let serialized = bincode::serialize(tx)?;
        let base64_tx = general_purpose::STANDARD.encode(serialized);
        
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                base64_tx,
                {
                    "encoding": "base64",
                    "preflightCommitment": "confirmed"
                }
            ]
        });

        // Use mainnet RPC URL directly to bypass client endpoint constraints
        let rpc_url = "https://api.mainnet-beta.solana.com";

        let response = self.http_client.post(rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| anyhow!("RPC send request failed: {}", e))?;

        if response.status().is_success() {
            let res_body: serde_json::Value = response.json().await?;
            if let Some(err) = res_body.get("error") {
                return Err(anyhow!("RPC error: {}", err));
            }
            let signature = res_body["result"]
                .as_str()
                .ok_or_else(|| anyhow!("No transaction signature returned from RPC"))?;
            Ok(signature.to_string())
        } else {
            let err_text = response.text().await?;
            Err(anyhow!("RPC HTTP error: {}", err_text))
        }
    }
}

#[derive(Debug, Clone)]
pub struct RbnRegistryEntry {
    pub operator: Pubkey,
    pub peer_id: String,
    pub multiaddresses: String,
    pub is_active: bool,
    pub last_registered: i64,
    pub stake_amount: u64,
    pub node_name: String,
}

impl RbnRegistryEntry {
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        if data.len() < 8 + 32 + 4 {
            return Err(anyhow!("Data too short"));
        }
        // Skip Anchor discriminator (8 bytes)
        let mut offset = 8;
        
        // 1. operator (32 bytes)
        let mut op_bytes = [0u8; 32];
        op_bytes.copy_from_slice(&data[offset..offset+32]);
        let operator = Pubkey::new_from_array(op_bytes);
        offset += 32;
        
        // 2. peer_id (String: u32 length + bytes)
        if data.len() < offset + 4 {
            return Err(anyhow!("Data truncated at peer_id length"));
        }
        let peer_id_len = u32::from_le_bytes(data[offset..offset+4].try_into()?) as usize;
        offset += 4;
        if data.len() < offset + peer_id_len {
            return Err(anyhow!("Data truncated at peer_id bytes"));
        }
        let peer_id = String::from_utf8(data[offset..offset+peer_id_len].to_vec())?;
        offset += peer_id_len;
        
        // 3. multiaddresses (String: u32 length + bytes)
        if data.len() < offset + 4 {
            return Err(anyhow!("Data truncated at multiaddresses length"));
        }
        let multiaddresses_len = u32::from_le_bytes(data[offset..offset+4].try_into()?) as usize;
        offset += 4;
        if data.len() < offset + multiaddresses_len {
            return Err(anyhow!("Data truncated at multiaddresses bytes"));
        }
        let multiaddresses = String::from_utf8(data[offset..offset+multiaddresses_len].to_vec())?;
        offset += multiaddresses_len;

        // 4. node_name (String: u32 length + bytes)
        // Check if node_name exists (for backward compatibility with older deployments)
        let mut node_name = "Unknown RBN".to_string();
        if data.len() >= offset + 4 {
            let node_name_len = u32::from_le_bytes(data[offset..offset+4].try_into()?) as usize;
            if data.len() >= offset + 4 + node_name_len {
                offset += 4;
                if let Ok(name_str) = String::from_utf8(data[offset..offset+node_name_len].to_vec()) {
                    node_name = name_str;
                }
                offset += node_name_len;
            }
        }
        
        // 5. is_active (bool: 1 byte)
        if data.len() < offset + 1 {
            return Err(anyhow!("Data truncated at is_active"));
        }
        let is_active = data[offset] != 0;
        offset += 1;
        
        // 6. last_registered (i64: 8 bytes)
        if data.len() < offset + 8 {
            return Err(anyhow!("Data truncated at last_registered"));
        }
        let last_registered = i64::from_le_bytes(data[offset..offset+8].try_into()?);
        offset += 8;
        
        // 7. stake_amount (u64: 8 bytes)
        if data.len() < offset + 8 {
            return Err(anyhow!("Data truncated at stake_amount"));
        }
        let stake_amount = u64::from_le_bytes(data[offset..offset+8].try_into()?);
        
        Ok(Self {
            operator,
            peer_id,
            multiaddresses,
            is_active,
            last_registered,
            stake_amount,
            node_name,
        })
    }
}

fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    use sha2::{Sha256, Digest};
    let preimage = format!("{}:{}", namespace, name);
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    let result = hasher.finalize();
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&result[..8]);
    discriminator
}

fn borsh_serialize_string(s: &str) -> Vec<u8> {
    let mut data = Vec::new();
    let len = s.len() as u32;
    data.extend_from_slice(&len.to_le_bytes());
    data.extend_from_slice(s.as_bytes());
    data
}
