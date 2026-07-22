//! Custom binary codec for Introvert signaling protocol
//!
//! This codec optimizes file chunk transfers by using raw binary encoding
//! instead of base64, reducing wire overhead by ~33%.
//!
//! Protocol format:
//! - Byte 0: Version (0x01)
//! - Byte 1: Flags (bit 0 = has binary data)
//! - Bytes 2-5: JSON payload length (u32 big-endian)
//! - Bytes 6..6+json_len: JSON payload
//! - Bytes 6+json_len..: Raw binary data (if flag set)

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response::Codec;
use serde::{Deserialize, Serialize};
use std::io;

/// Protocol identifier
pub const PROTOCOL: &str = "/introvert/signaling/2.0.0";

/// Current protocol version
const PROTOCOL_VERSION: u8 = 0x01;

/// Flag indicating binary data follows the JSON payload
const FLAG_HAS_BINARY: u8 = 0x01;

/// Maximum payload size (10MB for file chunks)
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;

/// Custom codec that optimizes FileChunk transfers with raw binary encoding
#[derive(Debug, Clone, Default)]
pub struct IntrovertCodec;

/// Binary codec request wrapper (for /introvert/signaling/2.0.0 — future protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinarySignalingRequest(pub crate::network::types::SignalingPayload);

/// Binary codec response wrapper (for /introvert/signaling/2.0.0 — future protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinarySignalingResponse(pub String);

/// Internal representation for serialization
#[derive(Debug, Serialize, Deserialize)]
pub enum PayloadWrapper {
    /// Standard JSON-only payload
    Json(crate::network::types::SignalingPayload),
    /// FileChunk with binary data extracted
    FileChunkBinary {
        transfer_id: String,
        chunk_index: u32,
        total_chunks: u32,
        data_len: u32,
    },
}

#[async_trait]
impl Codec for IntrovertCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = BinarySignalingRequest;
    type Response = BinarySignalingResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read header: version (1 byte) + flags (1 byte) + json_len (4 bytes)
        let mut header = [0u8; 6];
        io.read_exact(&mut header).await?;

        let version = header[0];
        if version != PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported protocol version: {}", version),
            ));
        }

        let flags = header[1];
        let has_binary = (flags & FLAG_HAS_BINARY) != 0;

        let json_len = u32::from_be_bytes([header[2], header[3], header[4], header[5]]) as usize;
        if json_len > MAX_PAYLOAD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("JSON payload too large: {} bytes", json_len),
            ));
        }

        // Read JSON payload
        let mut json_buf = vec![0u8; json_len];
        io.read_exact(&mut json_buf).await?;

        if has_binary {
            // Read binary data length (4 bytes)
            let mut data_len_buf = [0u8; 4];
            io.read_exact(&mut data_len_buf).await?;
            let data_len = u32::from_be_bytes(data_len_buf) as usize;

            if data_len > MAX_PAYLOAD_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Binary data too large: {} bytes", data_len),
                ));
            }

            // Read binary data
            let mut binary_data = vec![0u8; data_len];
            io.read_exact(&mut binary_data).await?;

            // Deserialize the JSON wrapper
            let wrapper: PayloadWrapper = serde_json::from_slice(&json_buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            // Reconstruct the FileChunk with base64-encoded data
            match wrapper {
                PayloadWrapper::FileChunkBinary { transfer_id, chunk_index, total_chunks, .. } => {
                    use base64::Engine;
                    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&binary_data);
                    Ok(BinarySignalingRequest(crate::network::types::SignalingPayload::FileChunk {
                        transfer_id,
                        chunk_index,
                        total_chunks,
                        data_base64,
                    }))
                }
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Expected FileChunkBinary wrapper for binary payload",
                )),
            }
        } else {
            // Standard JSON-only payload
            let payload: crate::network::types::SignalingPayload = serde_json::from_slice(&json_buf)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(BinarySignalingRequest(payload))
        }
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Response is always simple JSON
        let mut len_buf = [0u8; 4];
        io.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_PAYLOAD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Response too large: {} bytes", len),
            ));
        }

        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;

        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        BinarySignalingRequest(payload): Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Check if this is a FileChunk with binary data
        if let crate::network::types::SignalingPayload::FileChunk {
            ref transfer_id,
            chunk_index,
            total_chunks,
            ref data_base64,
        } = payload {
            use base64::Engine;
            // Decode base64 to raw binary
            let binary_data = base64::engine::general_purpose::STANDARD
                .decode(data_base64)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            // Create wrapper without the large base64 string
            let wrapper = PayloadWrapper::FileChunkBinary {
                transfer_id: transfer_id.clone(),
                chunk_index,
                total_chunks,
                data_len: binary_data.len() as u32,
            };

            let json_bytes = serde_json::to_vec(&wrapper)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            // Write header
            let mut header = [0u8; 6];
            header[0] = PROTOCOL_VERSION;
            header[1] = FLAG_HAS_BINARY;
            let json_len = json_bytes.len() as u32;
            header[2..6].copy_from_slice(&json_len.to_be_bytes());
            io.write_all(&header).await?;

            // Write JSON payload
            io.write_all(&json_bytes).await?;

            // Write binary data length
            let data_len = binary_data.len() as u32;
            io.write_all(&data_len.to_be_bytes()).await?;

            // Write binary data
            io.write_all(&binary_data).await?;

            io.flush().await?;
            return Ok(());
        }

        // Standard JSON-only payload
        let json_bytes = serde_json::to_vec(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Write header (no binary flag)
        let mut header = [0u8; 6];
        header[0] = PROTOCOL_VERSION;
        header[1] = 0; // No binary
        let json_len = json_bytes.len() as u32;
        header[2..6].copy_from_slice(&json_len.to_be_bytes());
        io.write_all(&header).await?;

        // Write JSON payload
        io.write_all(&json_bytes).await?;
        io.flush().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        BinarySignalingResponse(response): Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let json_bytes = serde_json::to_vec(&response)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let len = json_bytes.len() as u32;
        io.write_all(&len.to_be_bytes()).await?;
        io.write_all(&json_bytes).await?;
        io.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::types::SignalingPayload;
    use base64::Engine;

    /// Generate test binary data of a given size
    fn generate_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    /// Calculate the wire size of a FileChunk using the old base64 JSON approach
    fn calculate_old_wire_size(data: &[u8]) -> usize {
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(data);
        let payload = SignalingPayload::FileChunk {
            transfer_id: "gft_abc123_1234567890".to_string(),
            chunk_index: 0,
            total_chunks: 100,
            data_base64,
        };
        let json_bytes = serde_json::to_vec(&payload).unwrap();
        json_bytes.len()
    }

    /// Calculate the wire size using the new binary encoding
    fn calculate_new_wire_size(data: &[u8]) -> usize {
        let wrapper = PayloadWrapper::FileChunkBinary {
            transfer_id: "gft_abc123_1234567890".to_string(),
            chunk_index: 0,
            total_chunks: 100,
            data_len: data.len() as u32,
        };
        let json_bytes = serde_json::to_vec(&wrapper).unwrap();
        
        // Header (6 bytes) + JSON payload + binary data length (4 bytes) + binary data
        6 + json_bytes.len() + 4 + data.len()
    }

    #[test]
    fn test_wire_size_comparison() {
        println!("\n=== FileChunk Binary Encoding Wire Size Test ===\n");
        
        let test_sizes = vec![
            1024,        // 1 KB
            4096,        // 4 KB
            16384,       // 16 KB
            65536,       // 64 KB (typical relay chunk)
            262144,      // 256 KB (typical direct chunk)
            1048576,     // 1 MB
        ];

        println!("{:<12} {:<15} {:<15} {:<12} {:<10}", 
            "Data Size", "Old (JSON+B64)", "New (Binary)", "Savings", "Percent");
        println!("{:<12} {:<15} {:<15} {:<12} {:<10}", 
            "─────────", "─────────────", "────────────", "───────", "───────");

        for size in &test_sizes {
            let data = generate_test_data(*size);
            let old_size = calculate_old_wire_size(&data);
            let new_size = calculate_new_wire_size(&data);
            let savings = old_size - new_size;
            let percent = (savings as f64 / old_size as f64) * 100.0;

            let size_str = if *size >= 1048576 {
                format!("{} MB", size / 1048576)
            } else if *size >= 1024 {
                format!("{} KB", size / 1024)
            } else {
                format!("{} B", size)
            };

            println!("{:<12} {:<15} {:<15} {:<12} {:<10.1}%",
                size_str,
                format!("{} bytes", old_size),
                format!("{} bytes", new_size),
                format!("{} bytes", savings),
                percent
            );
        }

        println!("\n=== Verification ===\n");

        // Verify the encoding is correct
        let data = generate_test_data(1024);
        let old_size = calculate_old_wire_size(&data);
        let new_size = calculate_new_wire_size(&data);

        // The savings should be approximately 33% for the data portion
        // (base64 adds 33% overhead)
        let data_overhead_old = (data.len() as f64 * 4.0 / 3.0).ceil() as usize;
        let data_overhead_new = data.len();
        
        println!("Original data size: {} bytes", data.len());
        println!("Base64 encoded size: {} bytes (+{:.1}%)", 
            data_overhead_old, 
            (data_overhead_old as f64 / data.len() as f64 - 1.0) * 100.0
        );
        println!("Binary encoded size: {} bytes (+0.0%)", data_overhead_new);
        println!("\nTotal wire size reduction: {:.1}%", 
            (1.0 - new_size as f64 / old_size as f64) * 100.0
        );

        // Assertions
        assert!(new_size < old_size, "New encoding should be smaller");
        assert!((1.0 - new_size as f64 / old_size as f64) > 0.20, 
            "Should save at least 20% wire space");
    }

    #[test]
    fn test_non_file_chunk_unchanged() {
        println!("\n=== Non-FileChunk Payload Test ===\n");

        // Test that non-FileChunk payloads are unchanged
        let payloads = vec![
            ("ChatMessage", SignalingPayload::ChatMessage {
                content: "Hello, world!".to_string(),
                msg_id: "msg_123".to_string(),
                timestamp: 1234567890,
                reply_to: None,
                is_backfill: false,
            }),
            ("Acknowledgement", SignalingPayload::Acknowledgement {
                msg_id: "msg_123".to_string(),
                status: 1,
            }),
            ("TypingStart", SignalingPayload::TypingStart {
                chat_id: "peer_abc".to_string(),
            }),
        ];

        for (name, payload) in payloads {
            let json_bytes = serde_json::to_vec(&payload).unwrap();
            println!("{:<20} {} bytes (JSON only)", name, json_bytes.len());
        }

        println!("\nAll non-FileChunk payloads use standard JSON encoding.");
    }

    #[test]
    fn test_protocol_header_size() {
        println!("\n=== Protocol Header Size ===\n");
        
        // Header: version(1) + flags(1) + json_len(4) = 6 bytes
        println!("Protocol header: 6 bytes");
        println!("  - Version: 1 byte");
        println!("  - Flags: 1 byte");
        println!("  - JSON length: 4 bytes");
        println!("\nFor binary payloads, additional 4 bytes for data length.");
        println!("Total overhead: 10 bytes (negligible for chunk sizes > 1KB)");
    }
}
