use magic_wormhole::{Wormhole, transfer};
use anyhow::{Result, Context};
use crate::identity::SovereignIdentity;
use std::future::Future;
use async_compat::Compat;
use tracing::info;

/// Creates a new Wormhole invite, returning the human-readable code and a future that resolves to the peer's identity.
pub async fn create_invite(my_identity: SovereignIdentity) -> Result<(String, impl Future<Output = Result<SovereignIdentity>>)> {
    info!("Wormhole: Connecting to rendezvous relay...");
    crate::dispatch_debug_log("Wormhole: Contacting rendezvous relay...");
    
    let mut custom_config = transfer::APP_CONFIG.clone();
    custom_config.rendezvous_url = std::borrow::Cow::Borrowed("wss://relay.magic-wormhole.io/v1");
    
    crate::dispatch_debug_log("Wormhole: Initializing Compat block for invite...");
    
    // First, establish the connection with retries to get the code
    let mut connector = None;
    let mut last_err = None;
    for attempt in 1..=3 {
        let msg = format!("Wormhole: Connecting to rendezvous relay (attempt {}/3)...", attempt);
        info!("{}", msg);
        crate::dispatch_debug_log(&msg);
        
        match magic_wormhole::MailboxConnection::create(
            custom_config.clone(),
            2, // 2 words for the code
        ).await {
            Ok(c) => {
                crate::dispatch_debug_log("Wormhole: Mailbox connection established!");
                connector = Some(c);
                break;
            }
            Err(e) => {
                let err_msg = format!("Wormhole attempt {} failed: {}", attempt, e);
                info!("{}", err_msg);
                crate::dispatch_debug_log(&err_msg);
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        }
    }

    let connector = match connector {
        Some(c) => c,
        None => return Err(anyhow::anyhow!("Failed to connect to Wormhole relay after 3 attempts: {:?}", last_err)),
    };

    let code = connector.code().to_string();
    info!("Wormhole: Code generated: {}", code);
    crate::dispatch_debug_log(&format!("Wormhole: Code generated: {}", code));

    // Now wrap the handshake in Compat for compatibility
    let handshake_future = Compat::new(async move {
        let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
        
        // Mutual exchange: Send then Receive
        let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
        wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;

        let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
        let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;
        
        let _ = wormhole.close().await;
        Ok(peer_identity)
    });

    Ok((code, handshake_future))
}

/// Joins an existing Wormhole session using a code and returns the peer's identity.
pub async fn accept_invite(code: String, my_identity: SovereignIdentity) -> Result<impl Future<Output = Result<SovereignIdentity>>> {
    info!("Wormhole: Joining existing session...");
    crate::dispatch_debug_log("Wormhole: Joining existing session...");

    let mut custom_config = transfer::APP_CONFIG.clone();
    custom_config.rendezvous_url = std::borrow::Cow::Borrowed("wss://relay.magic-wormhole.io/v1");

    let connector = magic_wormhole::MailboxConnection::connect(
        custom_config,
        code.parse().context("Invalid Wormhole code format")?,
        false,
    ).await.context("Failed to join Wormhole session")?;

    let handshake_future = Compat::new(async move {
        let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
        
        // Mutual exchange: Receive then Send
        let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
        let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;

        let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
        wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;
        
        let _ = wormhole.close().await;
        Ok(peer_identity)
    });

    Ok(handshake_future)
}
