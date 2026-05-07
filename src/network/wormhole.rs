use magic_wormhole::{Wormhole, transfer};
use anyhow::{Result, Context};
use crate::identity::SovereignIdentity;
use std::future::Future;

/// Creates a new Wormhole invite, returning the human-readable code and a future that resolves to the peer's identity.
pub async fn create_invite(my_identity: SovereignIdentity) -> Result<(String, impl Future<Output = Result<SovereignIdentity>>)> {
    let connector = magic_wormhole::MailboxConnection::create(
        transfer::APP_CONFIG.clone(),
        2, // 2 words for the code
    ).await.context("Failed to connect to Wormhole relay")?;

    let code = connector.code().to_string();

    let handshake_future = async move {
        let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
        
        // Mutual exchange: Send then Receive
        let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
        wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;

        let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
        let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;
        
        let _ = wormhole.close().await;
        Ok(peer_identity)
    };

    Ok((code, handshake_future))
}

/// Joins an existing Wormhole session using a code and returns the peer's identity.
pub async fn accept_invite(code: String, my_identity: SovereignIdentity) -> Result<impl Future<Output = Result<SovereignIdentity>>> {
    let connector = magic_wormhole::MailboxConnection::connect(
        transfer::APP_CONFIG.clone(),
        code.parse().context("Invalid Wormhole code format")?,
        false, // use_secure_clipboard (legacy 0.7 param)
    ).await.context("Failed to join Wormhole session")?;

    let handshake_future = async move {
        let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
        
        // Mutual exchange: Receive then Send
        let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
        let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;

        let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
        wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;
        
        let _ = wormhole.close().await;
        Ok(peer_identity)
    };

    Ok(handshake_future)
}
