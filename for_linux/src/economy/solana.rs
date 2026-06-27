use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    signature::Keypair,
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
    rpc_client: Arc<RpcClient>,
    intr_mint: Pubkey,
    treasury_pubkey: Pubkey,
    treasury_api_url: String, // Endpoint for fee-payer co-signing
}

impl SolanaIncentiveEngine {
    pub fn new(rpc_url: &str, treasury_pubkey: &str, treasury_api_url: &str) -> Result<Self> {
        Ok(Self {
            rpc_client: Arc::new(RpcClient::new_with_timeout(rpc_url.to_string(), std::time::Duration::from_secs(3))),
            intr_mint: Pubkey::from_str("NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn")?,
            treasury_pubkey: Pubkey::from_str(treasury_pubkey)?,
            treasury_api_url: treasury_api_url.to_string(),
        })
    }

    pub fn get_treasury_pubkey(&self) -> Pubkey {
        self.treasury_pubkey
    }

    /// Fetches the native SOL balance (in lamports) for a given owner.
    pub async fn fetch_sol_balance(&self, owner: &Pubkey) -> Result<u64> {
        let balance = self.rpc_client.get_balance(owner).await?;
        Ok(balance)
    }

    /// Fetches the token balance for a given owner and mint.
    pub async fn fetch_token_balance(&self, owner: &Pubkey, mint: &Pubkey) -> Result<u64> {
        use solana_account_decoder::UiAccountEncoding;
        use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
        use solana_client::rpc_filter::{Memcmp, RpcFilterType};

        let filters = Some(vec![
            RpcFilterType::DataSize(165), // SPL Token account size
            RpcFilterType::Memcmp(Memcmp::new(
                32, // offset for owner
                solana_client::rpc_filter::MemcmpEncodedBytes::Base58(owner.to_string()),
            )),
            RpcFilterType::Memcmp(Memcmp::new(
                0, // offset for mint
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
            let data = match &account.data {
                solana_account_decoder::UiAccountData::Binary(b, _) => {
                    general_purpose::STANDARD.decode(b)?
                }
                _ => return Err(anyhow!("Unexpected account data format")),
            };

            if data.len() >= 72 {
                let mut amount_bytes = [0u8; 8];
                amount_bytes.copy_from_slice(&data[64..72]);
                Ok(u64::from_le_bytes(amount_bytes))
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }

    /// Fetches the INTR token balance for a given owner.
    pub async fn fetch_balance(&self, owner: &Pubkey) -> Result<u64> {
        self.fetch_token_balance(owner, &self.intr_mint).await
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
        let client = reqwest::Client::new();
        let payload = json!({
            "transaction": base64_tx,
        });

        let response = client.post(&self.treasury_api_url)
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
}
