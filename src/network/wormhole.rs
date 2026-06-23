use magic_wormhole::{Wormhole, transfer};
use anyhow::{Result, Context};
use crate::identity::SovereignIdentity;
use std::future::Future;
use async_compat::Compat;
use tracing::{debug, info};

/// Creates a new Wormhole invite, returning the human-readable code and a future that resolves to the peer's identity.
pub async fn create_invite(my_identity: SovereignIdentity) -> Result<(String, impl Future<Output = Result<SovereignIdentity>>)> {
    info!("Wormhole: Connecting to rendezvous relay...");
    crate::dispatch_debug_log("Wormhole: Contacting rendezvous relay...");
    
    let mut custom_config = transfer::APP_CONFIG.clone();
    custom_config.rendezvous_url = std::borrow::Cow::Borrowed("wss://relay.magic-wormhole.io/v1");

    crate::dispatch_debug_log("Wormhole: Initializing Compat block for invite...");
    let (code, handshake_future) = Compat::new(async move {
        let mut connector = None;
        let mut last_err = None;
        for attempt in 1..=3 {
            let msg = format!("Wormhole: Connecting to rendezvous relay (attempt {}/3)...", attempt);
            info!("{}", msg);
            crate::dispatch_debug_log(&msg);
            
            match magic_wormhole::MailboxConnection::create(
                custom_config.clone(),
                4, // 4 words for stronger security (~52 bits entropy)
            ).await {
                Ok(c) => {
                    crate::dispatch_debug_log("Wormhole: Mailbox connection established!");
                    connector = Some(c);
                    break;
                }
                Err(e) => {
                    let err_msg = format!("Wormhole attempt {} failed: {}", attempt, e);
                    debug!("{}", err_msg);
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
        let msg_code = format!("Wormhole: Code obtained: {}", code);
        info!("{}", msg_code);
        crate::dispatch_debug_log(&msg_code);

        let code_clone = code.clone();
        let handshake_future = async move {
            let msg_wait = format!("Wormhole: Code {} active. Waiting for peer...", code_clone);
            info!("{}", msg_wait);
            crate::dispatch_debug_log(&msg_wait);
            
            let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
            
            // Mutual exchange: Send then Receive
            crate::dispatch_debug_log("Wormhole: Peer connected. Exchanging keys...");
            let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
            wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;

            info!("Wormhole: Receiving peer identity...");
            let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
            let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;
            
            info!("Wormhole: Identity exchange complete with {}", peer_identity.peer_id);
            crate::dispatch_debug_log("Wormhole: Handover verified. Secure link established.");
            
            // Don't let close hang the whole process
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), wormhole.close()).await;
            
            Ok(peer_identity)
        };

        Ok((code, handshake_future))
    }).await?;

    crate::dispatch_debug_log("Wormhole: create_invite returning code to engine...");
    let handshake_compat = async move {
        Compat::new(handshake_future).await
    };

    Ok((code, handshake_compat))
}

/// Joins an existing Wormhole session using a code and returns the peer's identity.
pub async fn accept_invite(code: String, my_identity: SovereignIdentity) -> Result<impl Future<Output = Result<SovereignIdentity>>> {
    info!("Wormhole: Joining session with code {}...", code);
    crate::dispatch_debug_log(&format!("Wormhole: Joining session {}...", code));
    
    let mut custom_config = transfer::APP_CONFIG.clone();
    custom_config.rendezvous_url = std::borrow::Cow::Borrowed("wss://relay.magic-wormhole.io/v1");

    let parsed_code: magic_wormhole::Code = code.parse().context("Invalid Wormhole code format")?;

    let handshake_future = Compat::new(async move {
        let mut connector = None;
        let mut last_err = None;
        for attempt in 1..=3 {
            let msg = format!("Wormhole: Joining session (attempt {}/3)...", attempt);
            info!("{}", msg);
            crate::dispatch_debug_log(&msg);
            
            match magic_wormhole::MailboxConnection::connect(
                custom_config.clone(),
                parsed_code.clone(),
                false, // use_secure_clipboard (legacy 0.7 param)
            ).await {
                Ok(c) => {
                    crate::dispatch_debug_log("Wormhole: Joined mailbox successfully.");
                    connector = Some(c);
                    break;
                }
                Err(e) => {
                    let err_msg = format!("Wormhole join attempt {} failed: {}", attempt, e);
                    debug!("{}", err_msg);
                    crate::dispatch_debug_log(&err_msg);
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                }
            }
        }

        let connector = match connector {
            Some(c) => c,
            None => return Err(anyhow::anyhow!("Failed to join Wormhole session after 3 attempts: {:?}", last_err)),
        };

        let handshake_future = async move {
            crate::dispatch_debug_log("Wormhole: Linked to peer. Authenticating...");
            let mut wormhole = Wormhole::connect(connector).await.map_err(|e| anyhow::anyhow!("Wormhole connection failed: {}", e))?;
            
            // Mutual exchange: Receive then Send
            info!("Wormhole: Peer connected. Receiving identity...");
            let msg = wormhole.receive().await.map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;
            let peer_identity: SovereignIdentity = serde_json::from_slice(&msg).context("Invalid identity format from peer")?;

            info!("Wormhole: Sending our identity...");
            let my_id_bytes = serde_json::to_vec(&my_identity).context("Failed to serialize identity")?;
            wormhole.send(my_id_bytes).await.map_err(|e| anyhow::anyhow!("Failed to send: {}", e))?;
            
            info!("Wormhole: Session closed successfully.");
            crate::dispatch_debug_log("Wormhole: Success. Peer added to contacts.");
            
            // Don't let close hang
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), wormhole.close()).await;
            
            Ok(peer_identity)
        };

        Ok(handshake_future)
    }).await?;

    crate::dispatch_debug_log("Wormhole: accept_invite returning future to engine...");
    let handshake_compat = async move {
        Compat::new(handshake_future).await
    };

    Ok(handshake_compat)
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[tokio::test]
    async fn test_wss_relay() {
        let mut custom_config = magic_wormhole::transfer::APP_CONFIG.clone();
        custom_config.rendezvous_url = std::borrow::Cow::Borrowed("wss://relay.magic-wormhole.io/v1");
        println!("Attempting to connect to WSS relay on port 443...");
        let conn = magic_wormhole::MailboxConnection::create(
            custom_config,
            2,
        ).await;
        match conn {
            Ok(c) => {
                println!("SUCCESS! Code generated: {}", c.code());
            }
            Err(e) => {
                panic!("WSS Relay connection failed: {:?}", e);
            }
        }
    }
}

