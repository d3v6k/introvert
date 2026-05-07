use rusqlite::{params, Connection};
use anyhow::Result;
use std::path::Path;
use parking_lot::Mutex;
use sha2::{Sha256, Digest};
use chrono::Utc;

pub struct StorageService {
    conn: Mutex<Connection>,
}

impl StorageService {
    /// Creates a new SQLCipher encrypted database at the given path.
    pub fn new<P: AsRef<Path>>(path: P, key: &[u8; 32]) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Initialize SQLCipher encryption
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", format!("x'{}'", key_hex))?;

        let slf = Self { conn: Mutex::new(conn) };
        slf.bootstrap()?;
        Ok(slf)
    }

    /// Initializes the database schema.
    fn bootstrap(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                peer_id TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS mailbox_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recipient_hash BLOB NOT NULL,
                sender_peer_id TEXT NOT NULL,
                encrypted_payload BLOB NOT NULL,
                ttl_expiry INTEGER NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_mailbox_recipient ON mailbox_messages (recipient_hash);
            CREATE INDEX IF NOT EXISTS idx_mailbox_ttl ON mailbox_messages (ttl_expiry);
            CREATE TABLE IF NOT EXISTS mailbox_stats (
                date TEXT PRIMARY KEY,
                storage_bytes_seconds INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS crypto_sessions (
                session_id TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS contacts (
                peer_id TEXT PRIMARY KEY,
                static_key BLOB NOT NULL,
                solana_address TEXT NOT NULL,
                is_verified INTEGER DEFAULT 0,
                is_anchor_capable INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS session_cache (
                peer_id TEXT PRIMARY KEY,
                session_blob BLOB NOT NULL,
                last_active DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS reward_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                relayed_bytes INTEGER NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );"
        )?;
        Ok(())
    }

    /// Derives a truncated hash of a PeerId for zero-knowledge mailbox indexing.
    fn hash_peer_id(peer_id: &libp2p::PeerId) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(peer_id.to_bytes());
        hasher.finalize()[..16].to_vec()
    }

    /// Logs a relay event to the database.
    pub fn log_reward(&self, bytes: u64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO reward_log (relayed_bytes) VALUES (?1)",
            params![bytes],
        )?;
        Ok(())
    }

    /// Retrieves the total relayed bytes from the database.
    pub fn get_total_relayed_from_db(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT SUM(relayed_bytes) FROM reward_log")?;
        let total: Option<u64> = stmt.query_row([], |row| row.get(0))?;
        Ok(total.unwrap_or(0))
    }

    /// Persists an encrypted Noise session blob.
    pub fn save_session_state(&self, peer_id: &str, blob: Vec<u8>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO session_cache (peer_id, session_blob, last_active) 
             VALUES (?1, ?2, CURRENT_TIMESTAMP) 
             ON CONFLICT(peer_id) DO UPDATE SET 
                session_blob = excluded.session_blob, 
                last_active = CURRENT_TIMESTAMP",
            params![peer_id, blob],
        )?;
        Ok(())
    }

    /// Retrieves a persisted session blob for a peer.
    pub fn load_session_state(&self, peer_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT session_blob FROM session_cache WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| row.get::<_, Vec<u8>>(0))?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    /// Saves or updates a verified sovereign contact from the Wormhole handshake.
    pub fn upsert_sovereign_contact(&self, identity: &crate::identity::SovereignIdentity) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO contacts (peer_id, static_key, solana_address, is_verified, is_anchor_capable) 
             VALUES (?1, ?2, ?3, 1, ?4) 
             ON CONFLICT(peer_id) DO UPDATE SET 
                static_key = excluded.static_key, 
                solana_address = excluded.solana_address, 
                is_verified = 1,
                is_anchor_capable = excluded.is_anchor_capable",
            params![identity.peer_id, identity.static_key.to_vec(), identity.solana_address, identity.is_anchor_capable as i32],
        )?;
        Ok(())
    }

    /// Retrieves a sovereign contact by PeerId.
    pub fn get_contact(&self, peer_id: &str) -> Result<Option<crate::identity::SovereignIdentity>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, static_key, solana_address, is_anchor_capable FROM contacts WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            let static_key_vec: Vec<u8> = row.get(1)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                static_key,
                solana_address: row.get(2)?,
                is_anchor_capable: row.get::<_, i32>(3)? != 0,
            })
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    /// Removes a sovereign contact by PeerId.
    pub fn delete_contact(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM contacts WHERE peer_id = ?1", params![peer_id])?;
        // Also clean up any cached session or mailbox
        conn.execute("DELETE FROM session_cache WHERE peer_id = ?1", params![peer_id])?;
        let recipient_hash = Self::hash_peer_id(&peer_id.parse().unwrap_or(libp2p::PeerId::random()));
        conn.execute("DELETE FROM mailbox_messages WHERE recipient_hash = ?1", params![recipient_hash])?;
        Ok(())
    }

    /// Removes all sovereign contacts.
    pub fn clear_all_contacts(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM contacts", [])?;
        conn.execute("DELETE FROM session_cache", [])?;
        conn.execute("DELETE FROM mailbox_messages", [])?;
        Ok(())
    }

    /// Retrieves all verified sovereign contacts.
    pub fn get_all_contacts(&self) -> Result<Vec<crate::identity::SovereignIdentity>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, static_key, solana_address, is_anchor_capable FROM contacts WHERE is_verified = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(1)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                static_key,
                solana_address: row.get(2)?,
                is_anchor_capable: row.get::<_, i32>(3)? != 0,
            })
        })?;

        let mut contacts = Vec::new();
        for row in rows {
            contacts.push(row?);
        }
        Ok(contacts)
    }

    /// Fetches all verified contacts marked as Anchor Capable.
    pub fn fetch_all_anchor_nodes(&self) -> Result<Vec<crate::identity::SovereignIdentity>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, static_key, solana_address, is_anchor_capable FROM contacts WHERE is_verified = 1 AND is_anchor_capable = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(1)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                static_key,
                solana_address: row.get(2)?,
                is_anchor_capable: true,
            })
        })?;

        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row?);
        }
        Ok(nodes)
    }

    /// Stores a message for a peer that is currently offline (Anchor Service).
    pub fn store_mailbox_payload(&self, recipient: &libp2p::PeerId, sender: &libp2p::PeerId, payload: Vec<u8>) -> Result<()> {
        let conn = self.conn.lock();
        let ttl_expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() + (7 * 24 * 60 * 60); // 7 days TTL

        let recipient_hash = Self::hash_peer_id(recipient);

        conn.execute(
            "INSERT INTO mailbox_messages (recipient_hash, sender_peer_id, encrypted_payload, ttl_expiry) VALUES (?1, ?2, ?3, ?4)",
            params![recipient_hash, sender.to_string(), payload, ttl_expiry],
        )?;
        Ok(())
    }

    /// Retrieves and removes all pending messages for a specific peer.
    pub fn fetch_mailbox_payloads(&self, recipient: &libp2p::PeerId) -> Result<Vec<(String, Vec<u8>)>> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        
        let recipient_hash = Self::hash_peer_id(recipient);
        let mut messages = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT sender_peer_id, encrypted_payload FROM mailbox_messages WHERE recipient_hash = ?1")?;
            let rows = stmt.query_map(params![recipient_hash], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?;

            for row in rows {
                messages.push(row?);
            }
        }

        tx.execute("DELETE FROM mailbox_messages WHERE recipient_hash = ?1", params![recipient_hash])?;
        tx.commit()?;
        
        Ok(messages)
    }

    /// Retrieves and removes all pending messages for a specific peer, formatted for network transmission.
    pub fn drain_mailbox(&self, recipient: &libp2p::PeerId) -> Result<Vec<crate::network::MailboxMessage>> {
        let payloads = self.fetch_mailbox_payloads(recipient)?;
        Ok(payloads.into_iter().map(|(sender_id, payload)| {
            crate::network::MailboxMessage { sender_id, payload }
        }).collect())
    }

    /// Records mailbox storage usage (bytes-seconds) for the current day.
    pub fn record_mailbox_storage(&self, bytes: u64) -> Result<()> {
        let conn = self.conn.lock();
        let date = Utc::now().format("%Y-%m-%d").to_string();
        
        conn.execute(
            "INSERT INTO mailbox_stats (date, storage_bytes_seconds) 
             VALUES (?1, ?2) 
             ON CONFLICT(date) DO UPDATE SET 
                storage_bytes_seconds = storage_bytes_seconds + excluded.storage_bytes_seconds",
            params![date, bytes],
        )?;
        Ok(())
    }

    /// Purges expired mailbox messages (TTL Maintenance).
    pub fn cleanup_expired_mailbox(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let count = conn.execute("DELETE FROM mailbox_messages WHERE ttl_expiry < ?1", params![now])?;
        Ok(count)
    }

    /// Persists a message to the database. Thread-safe and suitable for background threads.
    pub fn store_message(&self, peer_id: &str, content: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO messages (peer_id, content) VALUES (?1, ?2)",
            params![peer_id, content],
        )?;
        Ok(())
    }

    /// Saves or updates a cryptographic session.
    pub fn save_session(&self, id: &str, data: &[u8]) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO crypto_sessions (session_id, data) VALUES (?1, ?2)",
            params![id, data],
        )?;
        Ok(())
    }
}
