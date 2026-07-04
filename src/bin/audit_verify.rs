use rusqlite::{params, Connection};
use anyhow::Result;
use introvert::identity::NodeIdentity;

fn main() -> Result<()> {
    let db_path = "tests/audit_manual.db";
    let seed = [0u8; 32];
    
    // 1. Derive keys
    let storage_key = NodeIdentity::derive_storage_key(seed)?;
    let session_enc_key = NodeIdentity::derive_session_encryption_key(seed)?;
    
    println!("Derived Storage Key: {}", hex::encode(storage_key));
    println!("Derived Session Enc Key: {}", hex::encode(session_enc_key));

    // 2. Open DB with SQLCipher
    let conn = Connection::open(db_path)?;
    let key_hex = hex::encode(storage_key);
    conn.pragma_update(None, "key", format!("x'{}'", key_hex))?;

    // 3. Setup schema and mock a session (as if the engine did it)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_cache (
            peer_id TEXT PRIMARY KEY,
            session_blob BLOB NOT NULL,
            last_active DATETIME DEFAULT CURRENT_TIMESTAMP
        );"
    )?;

    // Mock an encrypted blob
    let peer_id = "mock_peer";
    let raw_state = b"this is secret state data";
    
    // Encrypt it using the session_enc_key
    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
    let key = Key::<Aes256Gcm>::from_slice(&session_enc_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&[0u8; 12]);
    let encrypted = cipher.encrypt(nonce, raw_state.as_ref()).unwrap();
    
    conn.execute(
        "INSERT INTO session_cache (peer_id, session_blob) VALUES (?1, ?2)",
        params![peer_id, encrypted],
    )?;

    // 4. VERIFY: Try to read without session_enc_key
    let blob: Vec<u8> = conn.query_row(
        "SELECT session_blob FROM session_cache WHERE peer_id = ?1",
        params![peer_id],
        |row| row.get(0)
    )?;

    println!("Encrypted Blob from DB: {}", hex::encode(&blob));
    
    // Attempt to see if it's plain text (it shouldn't be)
    if let Ok(plain) = String::from_utf8(blob.clone()) {
        println!("WARNING: Blob is plain text: {}", plain);
    } else {
        println!("SUCCESS: Blob is NOT plain text (Encryption Active).");
    }

    // 5. VERIFY: Decrypt with key
    let decrypted = cipher.decrypt(nonce, blob.as_ref()).unwrap();
    println!("Decrypted Data: {}", String::from_utf8(decrypted)?);

    Ok(())
}
