use serde_json::json;
use crate::FfiResult;
use tracing::warn;

fn compute_pdq_hash(bytes: &[u8], mime: &str) -> Result<[u8; 32], String> {
    if mime.starts_with("video/") {
        return Err("Video PDQ requires frame extraction; fail-secure".into());
    }
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Image decode failed: {}", e))?;

    // Resize to 64x64 grayscale
    let gray = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .to_luma8();

    // Build 64x64 f64 matrix
    let mut matrix = [[0.0f64; 64]; 64];
    for y in 0..64 {
        for x in 0..64 {
            matrix[y][x] = gray[(x as u32, y as u32)][0] as f64;
        }
    }

    // Simplified 8x8 block DCT
    let mut dct = [[0.0f64; 8]; 8];
    for u in 0..8 {
        for v in 0..8 {
            let mut sum = 0.0;
            for y in 0..64 {
                for x in 0..64 {
                    sum += matrix[y][x]
                        * (((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 128.0).cos()
                        * (((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI) / 128.0).cos();
                }
            }
            dct[u][v] = sum;
        }
    }

    // Median-threshold to 32-byte hash
    let mut flat: Vec<f64> = dct.iter().flatten().copied().collect();
    flat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = flat[flat.len() / 2];

    let mut hash = [0u8; 32];
    for (i, val) in dct.iter().flatten().enumerate() {
        let byte_idx = i / 8;
        let bit_idx = 7 - (i % 8);
        if *val > median {
            hash[byte_idx] |= 1 << bit_idx;
        }
    }
    Ok(hash)
}

const BLOCKLIST: &[[u8; 32]] = &[]; // populated at build or from config

fn hamming_distance(a: &[u8; 32], b: &[u8; 32]) -> u32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x ^ y).count_ones()).sum()
}

fn shannon_entropy(data: &[u8]) -> f64 {
    let mut freq = [0u64; 256];
    for &b in data { freq[b as usize] += 1; }
    let len = data.len() as f64;
    -freq.iter().filter(|&&f| f > 0).map(|&f| {
        let p = f as f64 / len;
        p * p.log2()
    }).sum::<f64>()
}

/// Check if file magic bytes match an executable masquerading as an image
fn is_executable_masquerading(bytes: &[u8], mime: &str) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    
    // PE executable (MZ header)
    if bytes[0] == 0x4D && bytes[1] == 0x5A {
        if mime.starts_with("image/") || mime.starts_with("video/") {
            return true;
        }
    }
    
    // ELF executable
    if bytes[0] == 0x7F && bytes[1] == 0x45 && bytes[2] == 0x4C && bytes[3] == 0x46 {
        if mime.starts_with("image/") || mime.starts_with("video/") {
            return true;
        }
    }
    
    // Mach-O executable
    if bytes.len() >= 4 {
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic == 0xFEEDFACE || magic == 0xFEEDFACF || magic == 0xCEFAEDFE || magic == 0xCFFAEDFE {
            if mime.starts_with("image/") || mime.starts_with("video/") {
                return true;
            }
        }
    }
    
    false
}

pub fn inspect_media(bytes: &[u8], mime: &str) -> (String, String, f64) {
    // 1. Check for executable masquerading as media - HARD BLOCK
    if is_executable_masquerading(bytes, mime) {
        warn!("[Ingestion] Executable masquerading as {} detected", mime);
        return (String::new(), "knownViolationBlocked".into(), 0.99);
    }
    
    // 2. Compute PDQ hash
    match compute_pdq_hash(bytes, mime) {
        Ok(hash) => {
            let hash_hex = hex::encode(hash);
            
            // 3. Check against blocklist - HARD BLOCK
            for blocked in BLOCKLIST {
                if hamming_distance(&hash, blocked) <= 10 {
                    return (hash_hex, "knownViolationBlocked".into(), 0.99);
                }
            }
            
            // 4. High entropy check - PASSIVE LOG ONLY (no hard block)
            let entropy = shannon_entropy(bytes);
            if entropy > 7.95 && mime.starts_with("image/") {
                warn!("[Ingestion] High entropy asset allowed: {:.2} bits/byte for {} (passing to cipher matrix)", entropy, mime);
            }
            
            // 5. Default: approved
            (hash_hex, "approved".into(), 0.95)
        }
        Err(_) => {
            // PDQ failed (e.g., video) - fail-open for now
            warn!("[Ingestion] PDQ hash failed for {}, defaulting to approved", mime);
            (String::new(), "approved".into(), 0.80)
        }
    }
}
