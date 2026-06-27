use sha2::{Sha256, Digest};
fn main() {
    let mut hasher = Sha256::new();
    hasher.update(b"hello world");
    let file_hash = format!("{:x}", hasher.finalize());
    println!("Hash: {}", file_hash);
    println!("Len: {}", file_hash.len());
}
