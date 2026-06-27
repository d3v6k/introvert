use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use anyhow::Result;
use tracing::{debug, info};

/// Starts the local TCP loopback listener that tunnels all traffic via WebSockets to the RBN node.
pub async fn start_tunnel_client(local_port: u16, remote_ws_url: String) -> Result<(u16, tokio::task::JoinHandle<Result<()>>)> {
    let addr = SocketAddr::from(([127, 0, 0, 1], local_port));
    let listener = TcpListener::bind(addr).await?;
    let assigned_port = listener.local_addr()?.port();
    
    let handle = tokio::spawn(async move {
        info!("[Tunnel Client] Listening on loopback 127.0.0.1:{} tunneling to {}", assigned_port, remote_ws_url);
        
        loop {
            match listener.accept().await {
                Ok((tcp_stream, peer_addr)) => {
                    info!("[Tunnel Client] Accepted local connection from {}", peer_addr);
                    let url_clone = remote_ws_url.clone();
                    tokio::spawn(async move {
                        if let Err(e) = bridge_tcp_to_ws(tcp_stream, url_clone).await {
                            debug!("[Tunnel Client] Bridge failed: {}", e);
                        }
                    });
                }
                Err(e) => {
                    debug!("[Tunnel Client] Accept error: {}", e);
                }
            }
        }
    });
    Ok((assigned_port, handle))
}

async fn bridge_tcp_to_ws(tcp_stream: TcpStream, remote_ws_url: String) -> Result<()> {
    // 1. Establish Secure/Standard WebSocket connection
    let (ws_stream, _) = connect_async(remote_ws_url).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

    // 2. Spawn concurrent bidirectional proxying
    let ws_to_tcp = tokio::spawn(async move {
        while let Some(msg_res) = ws_read.next().await {
            match msg_res {
                Ok(Message::Binary(data)) => {
                    use tokio::io::AsyncWriteExt;
                    if tcp_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    let tcp_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 16384];
        loop {
            use tokio::io::AsyncReadExt;
            match tcp_read.read(&mut buf).await {
                Ok(0) => {
                    let _ = ws_write.send(Message::Close(None)).await;
                    break;
                }
                Ok(n) => {
                    let payload = buf[..n].to_vec();
                    if ws_write.send(Message::Binary(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(ws_to_tcp, tcp_to_ws);
    Ok(())
}

/// Starts the RBN daemon-side WebSocket server that accepts tunnel connections and proxies them to the local libp2p port.
pub fn start_tunnel_server(listen_port: u16, local_libp2p_port: u16) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        // SECURITY: Bind to localhost only — prevents external access to WebSocket tunnel
        let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));
        let listener = TcpListener::bind(addr).await?;
        info!("[Tunnel Server] Listening on {} proxying to local libp2p port {}", addr, local_libp2p_port);

        loop {
            match listener.accept().await {
                Ok((tcp_stream, peer_addr)) => {
                    tokio::spawn(async move {
                        if let Err(e) = handle_incoming_ws_tunnel(tcp_stream, local_libp2p_port).await {
                            debug!("[Tunnel Server] Failed to handle tunnel from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    debug!("[Tunnel Server] Accept error: {}", e);
                }
            }
        }
    })
}

async fn handle_incoming_ws_tunnel(tcp_stream: TcpStream, local_libp2p_port: u16) -> Result<()> {
    // 1. Upgrade incoming TCP stream to WebSocket
    let ws_stream = tokio_tungstenite::accept_async(tcp_stream).await?;
    
    // 2. Connect to the local libp2p node listener via TCP
    let local_dest = SocketAddr::from(([127, 0, 0, 1], local_libp2p_port));
    let local_libp2p_stream = TcpStream::connect(local_dest).await?;

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (mut tcp_read, mut tcp_write) = local_libp2p_stream.into_split();

    // 3. Bidirectional proxy
    let ws_to_tcp = tokio::spawn(async move {
        while let Some(msg_res) = ws_read.next().await {
            match msg_res {
                Ok(Message::Binary(data)) => {
                    use tokio::io::AsyncWriteExt;
                    if tcp_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    let tcp_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 16384];
        loop {
            use tokio::io::AsyncReadExt;
            match tcp_read.read(&mut buf).await {
                Ok(0) => {
                    let _ = ws_write.send(Message::Close(None)).await;
                    break;
                }
                Ok(n) => {
                    let payload = buf[..n].to_vec();
                    if ws_write.send(Message::Binary(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(ws_to_tcp, tcp_to_ws);
    Ok(())
}
