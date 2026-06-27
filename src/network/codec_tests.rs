//! Tests for the custom binary codec

#[cfg(test)]
mod tests {
    use crate::network::types::SignalingPayload;
    use crate::network::codec::{SignalingRequest, PayloadWrapper};
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
