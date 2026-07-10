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
use zeroize::Zeroize;
use serde_json::json;
use base64::{Engine as _, engine::general_purpose};
use std::sync::Arc;
use tracing::{debug, error, info};

pub struct SolanaIncentiveEngine {
    pub rpc_client: Arc<RpcClient>,
    http_client: reqwest::Client,
    intr_mint: Pubkey,
    treasury_pubkey: Pubkey,
    treasury_api_url: String, // Endpoint for fee-payer co-signing
    ipc_secret: [u8; 32],     // HMAC-SHA256 key loaded from /etc/introvert/ipc.secret
}

impl Drop for SolanaIncentiveEngine {
    fn drop(&mut self) {
        self.ipc_secret.zeroize();
    }
}

impl SolanaIncentiveEngine {
    pub fn new(rpc_url: &str, treasury_pubkey: &str, treasury_api_url: &str) -> Result<Self> {
        let ipc_secret = Self::load_ipc_secret().unwrap_or_else(|e| {
            tracing::warn!("[Economy] IPC secret unavailable: {}. Treasury relay disabled.", e);
            [0u8; 32]
        });
        Ok(Self {
            rpc_client: Arc::new(RpcClient::new_with_timeout(rpc_url.to_string(), std::time::Duration::from_secs(3))),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            intr_mint: Pubkey::from_str("EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf")?,
            treasury_pubkey: Pubkey::from_str(treasury_pubkey)?,
            treasury_api_url: treasury_api_url.to_string(),
            ipc_secret,
        })
    }

    /// Loads the 64-character hex IPC secret from /etc/introvert/ipc.secret.
    /// Falls back to a zeroed key for desktop/dev environments where the file
    /// doesn't exist. The RBN server always has this file; desktop clients don't
    /// need it since they don't relay to the treasury daemon directly.
    fn load_ipc_secret() -> Result<[u8; 32]> {
        let path = std::path::Path::new("/etc/introvert/ipc.secret");
        match std::fs::read_to_string(path) {
            Ok(hex_str) => {
                let hex_str = hex_str.trim();
                if hex_str.len() != 64 {
                    return Err(anyhow!("IPC secret must be exactly 64 hex characters (32 bytes), got {}", hex_str.len()));
                }
                let bytes = hex::decode(hex_str)
                    .map_err(|e| anyhow!("IPC secret is not valid hex: {}", e))?;
                let mut secret = [0u8; 32];
                secret.copy_from_slice(&bytes);
                Ok(secret)
            }
            Err(_) => {
                tracing::warn!("[Economy] IPC secret not found at {} — treasury relay auth disabled", path.display());
                Err(anyhow!("IPC secret file not found at {}", path.display()))
            }
        }
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

    /// Verifies a claimed prestige tier against the actual on-chain INTR balance.
    /// Returns the verified tier (0-4) based on real balance, not the claimed tier.
    /// This prevents peers from self-asserting inflated tiers for reward multiplier abuse.
    ///
    /// Tier thresholds (from whitepaper):
    ///   0 = Citizen (< 100k INTR)
    ///   1 = Sentinel (>= 100k INTR)
    ///   2 = Silver (>= 250k INTR)
    ///   3 = Gold (>= 500k INTR)
    ///   4 = Platinum (>= 1M INTR)
    pub async fn verify_prestige_tier(&self, owner: &Pubkey, claimed_tier: u8) -> Result<u8> {
        let balance_nano = self.fetch_balance(owner).await?;
        let balance_intr = balance_nano as f64 / 1_000_000_000.0;

        let verified_tier = if balance_intr >= 1_000_000.0 {
            4 // Platinum
        } else if balance_intr >= 500_000.0 {
            3 // Gold
        } else if balance_intr >= 250_000.0 {
            2 // Silver
        } else if balance_intr >= 100_000.0 {
            1 // Sentinel
        } else {
            0 // Citizen
        };

        if verified_tier != claimed_tier {
            tracing::warn!(
                "[Security] Prestige tier mismatch for {}: claimed={}, verified={} (balance={:.4} INTR)",
                owner, claimed_tier, verified_tier, balance_intr
            );
        }

        Ok(verified_tier)
    }

    /// Fetches the node's tier profile based on on-chain INTR balance.
    /// Returns (advanced_diagnostic_ui_visible, allocation_multiplier) for the Event 9 payload.
    ///
    /// Tier mapping:
    ///   < 50,000 INTR  → (false, 1.0)
    ///   >= 50,000      → (true, 1.5)  Sentinel
    ///   >= 100,000     → (true, 1.75) Silver
    ///   >= 250,000     → (true, 2.0)  Gold
    ///   >= 500,000     → (true, 2.5)  Platinum
    pub async fn fetch_node_tier_profile(&self, owner: &Pubkey) -> (bool, f32) {
        let balance_nano = self.fetch_balance(owner).await.unwrap_or(0);
        let balance_intr = balance_nano as f64 / 1_000_000_000.0;

        if balance_intr >= 500_000.0 {
            (true, 2.5)
        } else if balance_intr >= 250_000.0 {
            (true, 2.0)
        } else if balance_intr >= 100_000.0 {
            (true, 1.75)
        } else if balance_intr >= 50_000.0 {
            (true, 1.5)
        } else {
            (false, 1.0)
        }
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

    /// Relay a co-signing request to the treasury API with HMAC-SHA256 authentication.
    /// The timestamp is bound to the payload to prevent replay attacks.
    async fn relay_to_treasury(&self, base64_tx: String) -> Result<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        // Bind timestamp to payload — prevents replay attacks
        let message = format!("{}.{}", timestamp, base64_tx);

        let mut mac = HmacSha256::new_from_slice(&self.ipc_secret)
            .map_err(|e| anyhow!("HMAC init failed: {}", e))?;
        mac.update(message.as_bytes());
        let signature_hex = hex::encode(mac.finalize().into_bytes());

        let payload = json!({
            "transaction": base64_tx,
            "timestamp": timestamp,
        });

        let response = self.http_client.post(&self.treasury_api_url)
            .header("X-Introvert-Timestamp", &timestamp)
            .header("X-Introvert-Signature", &signature_hex)
            .json(&payload)
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

    /// Submits a reward claim and waits for on-chain confirmation before returning.
    /// This prevents phantom claims where the local state commits but the transaction
    /// never finalizes on Solana (e.g., dropped from mempool, slippage, congestion).
    ///
    /// Returns Ok(signature) only after the transaction is confirmed on-chain.
    /// Returns Err if the transaction fails or times out (30s).
    pub async fn submit_and_verify_reward_claim(&self, user_keypair: &Keypair, proof: &[u8]) -> Result<String> {
        // 1. Submit the claim
        let signature_str = self.submit_reward_claim(user_keypair, proof).await?;

        // 2. Poll for on-chain confirmation (max 30s, check every 3s)
        let signature = signature_str.parse::<solana_sdk::signature::Signature>()
            .map_err(|e| anyhow!("Invalid signature format: {}", e))?;

        for attempt in 0..10 {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

            match self.rpc_client.get_signature_status(&signature).await {
                Ok(Some(Ok(_))) => {
                    info!("[Economy] Transaction {} confirmed on-chain (attempt {})", signature_str, attempt + 1);
                    return Ok(signature_str);
                }
                Ok(Some(Err(e))) => {
                    error!("[Economy] Transaction {} failed on-chain: {:?}", signature_str, e);
                    return Err(anyhow!("Transaction failed on-chain: {:?}", e));
                }
                Ok(None) => {
                    debug!("[Economy] Transaction {} not yet seen (attempt {}/10)", signature_str, attempt + 1);
                }
                Err(e) => {
                    debug!("[Economy] RPC error checking status: {:?}", e);
                }
            }
        }

        // Timeout — transaction may still confirm later, but we can't hold the caller
        error!("[Economy] Transaction {} confirmation timeout after 30s", signature_str);
        Err(anyhow!("Transaction confirmation timeout: {}", signature_str))
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
        tracing::info!("[SolanaRegistry] Submitting {} transaction to Solana...", tx_type);
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

        // Use the configured RPC client endpoint (Mainnet-Beta by default)
        let rpc_url = self.rpc_client.url();

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

    /// Verifies that an operator has sufficient on-chain stake in the escrow PDA.
    /// Queries the EscrowState account derived from seeds = [b"escrow_state", operator].
    /// Returns (staked_amount_nano, is_unbonding).
    pub async fn verify_operator_stake(
        &self,
        operator: &Pubkey,
        registry_program_id: &Pubkey,
    ) -> Result<(u64, bool)> {
        let (escrow_pda, _) = Pubkey::find_program_address(
            &[b"escrow_state", operator.as_ref()],
            registry_program_id,
        );

        match self.rpc_client.get_account(&escrow_pda).await {
            Ok(account) => {
                if account.data.len() < 8 + 32 + 8 + 8 + 1 + 8 {
                    return Err(anyhow!("EscrowState account data too short"));
                }
                // Skip Anchor discriminator (8 bytes)
                let mut offset = 8;
                // operator: Pubkey (32 bytes)
                offset += 32;
                // staked_amount: u64
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&account.data[offset..offset + 8]);
                let staked_amount = u64::from_le_bytes(bytes);
                offset += 8;
                // last_staked_at: i64
                offset += 8;
                // is_unbonding: bool
                let is_unbonding = account.data[offset] != 0;

                Ok((staked_amount, is_unbonding))
            }
            Err(_) => Ok((0, false)), // No escrow account = no stake
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
