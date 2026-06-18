use rusqlite::{params, Connection};
use anyhow::Result;
use std::path::Path;
use parking_lot::Mutex;
use sha2::{Sha256, Digest};
use chrono::Utc;

pub struct StorageService {
    conn: Mutex<Connection>,
    _is_ephemeral: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DriveFileMetadata {
    pub filename: String,
    pub file_hash: String,
    pub mime_type: String,
    pub total_size: i64,
    pub local_path: String,
    pub is_backed_up: bool,
    pub timestamp: String,
}

pub struct GroupMeshInfo {
    pub group_id: String,
    pub name: String,
    pub members_json: String,
    pub secret: [u8; 32],
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingGroupInvite {
    pub group_id: String,
    pub name: String,
    pub description: String,
    pub inviter_peer_id: String,
    pub group_secret_wrapped: Vec<u8>,
    pub members_json: String,
}

impl StorageService {
    /// Creates a new SQLCipher encrypted database at the given path.
    pub fn new<P: AsRef<Path>>(path: P, key: &[u8; 32]) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Initialize SQLCipher encryption
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", format!("x'{}'", key_hex))?;

        let slf = Self { conn: Mutex::new(conn), _is_ephemeral: false };
        slf.bootstrap()?;
        Ok(slf)
    }

    /// Creates a memory-only non-encrypted database for stress testing.
    pub fn new_ephemeral() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let slf = Self { conn: Mutex::new(conn), _is_ephemeral: true };
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
                msg_id TEXT UNIQUE,
                content TEXT NOT NULL,
                reply_to_msg_id TEXT,
                is_me INTEGER DEFAULT 0,
                status INTEGER DEFAULT 0,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_messages_peer_time ON messages (peer_id, timestamp DESC);
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
                p2p_pubkey BLOB NOT NULL,
                static_key BLOB NOT NULL,
                solana_address TEXT NOT NULL,
                global_name TEXT,
                local_alias TEXT,
                avatar_base64 TEXT,
                is_verified INTEGER DEFAULT 0,
                is_incoming INTEGER DEFAULT 0,
                is_anchor_capable INTEGER DEFAULT 0,
                handle TEXT,
                last_seen INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS profile (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                name TEXT,
                handle TEXT UNIQUE,
                avatar_base64 TEXT,
                privacy_mode INTEGER DEFAULT 1
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
            );
            CREATE TABLE IF NOT EXISTS economy_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS groups (
                group_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                members_json TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS group_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                group_id TEXT NOT NULL,
                sender_id TEXT NOT NULL,
                msg_id TEXT UNIQUE,
                content TEXT NOT NULL,
                reply_to_msg_id TEXT,
                is_me INTEGER DEFAULT 0,
                status INTEGER DEFAULT 1,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_group_messages_id ON group_messages (group_id, timestamp DESC);
            CREATE TABLE IF NOT EXISTS drive_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                filename TEXT NOT NULL,
                file_hash TEXT NOT NULL UNIQUE,
                mime_type TEXT,
                total_size INTEGER NOT NULL,
                local_path TEXT,
                is_backed_up INTEGER DEFAULT 0,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS handle_registry (
                handle TEXT PRIMARY KEY,
                peer_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                signatures_json TEXT NOT NULL,
                verified INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS push_tokens (
                peer_id TEXT PRIMARY KEY,
                device_type TEXT NOT NULL,
                push_token TEXT NOT NULL,
                last_seen DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS message_reactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                msg_id TEXT NOT NULL,
                sender_id TEXT NOT NULL,
                emoji TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(msg_id, sender_id)
            );
            CREATE INDEX IF NOT EXISTS idx_drive_hash ON drive_files (file_hash);
            CREATE TABLE IF NOT EXISTS mesh_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_hash TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                data BLOB NOT NULL,
                timestamp INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mesh_chunks_file ON mesh_chunks (file_hash);
            CREATE INDEX IF NOT EXISTS idx_mesh_chunks_timestamp ON mesh_chunks (timestamp);
            CREATE TABLE IF NOT EXISTS group_secrets (
                group_id TEXT PRIMARY KEY,
                secret_blob BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS deleted_groups (
                group_id TEXT PRIMARY KEY,
                deleted_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS _schema_version (version INTEGER PRIMARY KEY);"
        )?;

        // Notes table
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT DEFAULT '[]',
                image_path TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )", []
        )?;
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes (updated_at DESC)", []);

        // Notes version history
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS note_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                note_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT DEFAULT '[]',
                version_number INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
            )", []
        )?;
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_note_versions ON note_versions (note_id, version_number DESC)", []);

        // Call history
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS call_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                peer_id TEXT NOT NULL,
                call_type TEXT NOT NULL,
                media_type INTEGER NOT NULL,
                duration_seconds INTEGER DEFAULT 0,
                is_incoming BOOLEAN NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )", []
        )?;
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_call_history_peer ON call_history (peer_id, timestamp DESC)", []);

        // Migrations
        let _ = conn.execute("ALTER TABLE profile ADD COLUMN handle TEXT", []);
        let _ = conn.execute("ALTER TABLE profile ADD COLUMN privacy_mode INTEGER DEFAULT 1", []);
        if let Ok(1) = conn.execute("INSERT OR IGNORE INTO economy_meta (key, value) VALUES ('privacy_default_migrated', 'true')", []) {
            let _ = conn.execute("UPDATE profile SET privacy_mode = 1", []);
        }
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN reply_to_msg_id TEXT", []);
        let _ = conn.execute("ALTER TABLE group_messages ADD COLUMN reply_to_msg_id TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN global_name TEXT", []);
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS handle_registry (handle TEXT PRIMARY KEY, peer_id TEXT, timestamp INTEGER, signatures_json TEXT, verified INTEGER DEFAULT 0)", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN local_alias TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN avatar_base64 TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN p2p_pubkey BLOB", []);
        let _ = conn.execute("ALTER TABLE profile ADD COLUMN avatar_base64 TEXT", []);
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN msg_id TEXT", []);
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN is_me INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN status INTEGER DEFAULT 0", []);
        let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_msg_id ON messages (msg_id)", []);
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_peer_time ON messages (peer_id, timestamp DESC)", []);
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS economy_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []);
        let _ = conn.execute("ALTER TABLE groups ADD COLUMN description TEXT DEFAULT ''", []);
        let _ = conn.execute("ALTER TABLE group_messages ADD COLUMN is_me INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE group_messages ADD COLUMN status INTEGER DEFAULT 1", []);
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS deleted_groups (group_id TEXT PRIMARY KEY, deleted_at DATETIME DEFAULT CURRENT_TIMESTAMP)", []);
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS push_tokens (peer_id TEXT PRIMARY KEY, device_type TEXT NOT NULL, push_token TEXT NOT NULL, last_seen DATETIME DEFAULT CURRENT_TIMESTAMP)", []);
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS message_reactions (id INTEGER PRIMARY KEY AUTOINCREMENT, msg_id TEXT NOT NULL, sender_id TEXT NOT NULL, emoji TEXT NOT NULL, timestamp DATETIME DEFAULT CURRENT_TIMESTAMP, UNIQUE(msg_id, sender_id))", []);
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_reactions_msg ON message_reactions (msg_id)", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN retention_hours INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE groups ADD COLUMN retention_hours INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE groups ADD COLUMN muted_members_json TEXT DEFAULT '[]'", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN handle TEXT", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN is_incoming INTEGER DEFAULT 0", []);

        Ok(())
    }

    /// Checks if the initial Seed Balance has been claimed.
    pub fn is_anchor_mode_enabled(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'anchor_mode'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val == "true";
                }
            }
        }
        false
    }

    /// Gets the Intro-Claw AI mode: 0 = Offline (Deterministic), 1 = Hybrid AI Assistant.
    pub fn get_intro_claw_ai_mode(&self) -> i32 {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'intro_claw_ai_mode'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val.parse::<i32>().unwrap_or(0);
                }
            }
        }
        0 // Default: 100% Offline
    }

    /// Sets the Intro-Claw AI mode (0 = Offline, 1 = Hybrid) and optionally the encrypted API key.
    pub fn set_intro_claw_ai_mode(&self, mode: i32, api_key: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('intro_claw_ai_mode', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![mode.to_string()],
        )?;
        if !api_key.is_empty() {
            // Encrypt the API key using the SQLCipher master key (already active on this connection)
            conn.execute(
                "INSERT INTO economy_meta (key, value) VALUES ('intro_claw_api_key', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![api_key],
            )?;
        }
        Ok(())
    }

    /// Gets the Intro-Claw API key (stored encrypted via SQLCipher).
    pub fn get_intro_claw_api_key(&self) -> String {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'intro_claw_api_key'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val;
                }
            }
        }
        String::new()
    }

    pub fn is_privacy_mode_extroverted(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT privacy_mode FROM profile WHERE id = 1") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, i32>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val == 1;
                }
            }
        }
        true
    }


    pub fn get_conn(&self) -> &Mutex<Connection> {
        &self.conn
    }

    /// Sets the Seed Balance claim status.
    pub fn set_seed_claimed(&self, claimed: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('seed_claimed', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![claimed.to_string()],
        )?;
        Ok(())
    }

    pub fn is_seed_claimed(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'seed_claimed'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val == "true";
                }
            }
        }
        false
    }

    /// Sets the anchor node mode status.
    pub fn set_anchor_mode_enabled(&self, enabled: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('anchor_mode', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![enabled.to_string()],
        )?;
        Ok(())
    }

    /// Checks if tunnel mode is enabled in configuration.
    pub fn is_tunnel_mode_enabled(&self) -> bool {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare("SELECT value FROM economy_meta WHERE key = 'tunnel_mode'") {
            Ok(s) => s,
            Err(_) => return false,
        };
        let mut rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return false,
        };
        if let Some(Ok(val)) = rows.next() {
            val == "true"
        } else {
            false
        }
    }

    /// Sets the tunnel mode status.
    pub fn set_tunnel_mode_enabled(&self, enabled: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('tunnel_mode', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![enabled.to_string()],
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

    /// Deletes a persisted session blob for a peer.
    pub fn delete_session_state(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM session_cache WHERE peer_id = ?1", params![peer_id])?;
        Ok(())
    }

    /// Saves or updates a verified sovereign contact from the Wormhole handshake.
    pub fn upsert_sovereign_contact(&self, identity: &crate::identity::SovereignIdentity, is_verified: bool, is_incoming: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO contacts (peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_verified, is_incoming, is_anchor_capable, handle) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) 
             ON CONFLICT(peer_id) DO UPDATE SET 
                p2p_pubkey = excluded.p2p_pubkey,
                static_key = excluded.static_key, 
                solana_address = excluded.solana_address, 
                global_name = excluded.global_name,
                local_alias = excluded.local_alias,
                avatar_base64 = excluded.avatar_base64,
                is_verified = excluded.is_verified,
                is_incoming = excluded.is_incoming,
                is_anchor_capable = excluded.is_anchor_capable,
                handle = excluded.handle",
            params![
                identity.peer_id, 
                identity.p2p_pubkey,
                identity.static_key.to_vec(), 
                identity.solana_address, 
                identity.global_name,
                identity.local_alias,
                identity.avatar_base64,
                if is_verified { 1 } else { 0 },
                if is_incoming { 1 } else { 0 },
                identity.is_anchor_capable as i32,
                identity.handle
            ],
        )?;
        Ok(())
    }

    pub fn update_contact_verification(&self, peer_id: &str, is_verified: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE contacts SET is_verified = ?1 WHERE peer_id = ?2", params![if is_verified { 1 } else { 0 }, peer_id])?;
        Ok(())
    }

    pub fn get_contact_status(&self, peer_id: &str) -> Result<Option<(bool, bool)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT is_verified, is_incoming FROM contacts WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            Ok((row.get::<_, i32>(0)? != 0, row.get::<_, i32>(1)? != 0))
        })?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn is_contact_verified(&self, peer_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT is_verified FROM contacts WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| row.get::<_, i32>(0))?;
        if let Some(row) = rows.next() {
            Ok(row? != 0)
        } else {
            Ok(false)
        }
    }

    /// Retrieves a sovereign contact by PeerId.
    pub fn get_contact(&self, peer_id: &str) -> Result<Option<crate::identity::SovereignIdentity>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle FROM contacts WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                p2p_pubkey: row.get(1)?,
                static_key,
                solana_address: row.get(3)?,
                global_name: row.get(4)?,
                local_alias: row.get(5)?,
                avatar_base64: row.get(6)?,
                is_anchor_capable: row.get::<_, i32>(7)? != 0,
                retention_seconds: row.get(8)?,
                handle: row.get(9)?,
            })
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    /// Removes a sovereign contact by PeerId and all associated data.
    pub fn delete_contact(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM contacts WHERE peer_id = ?1", params![peer_id])?;
        // Also clean up any cached session, mailbox, and messages
        conn.execute("DELETE FROM session_cache WHERE peer_id = ?1", params![peer_id])?;
        conn.execute("DELETE FROM messages WHERE peer_id = ?1", params![peer_id])?;
        
        let recipient_hash = Self::hash_peer_id(&peer_id.parse().unwrap_or(libp2p::PeerId::random()));
        conn.execute("DELETE FROM mailbox_messages WHERE recipient_hash = ?1", params![recipient_hash])?;
        Ok(())
    }

    /// Removes all messages for a specific peer (Deletes chat history).
    pub fn delete_chat(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE peer_id = ?1", params![peer_id])?;
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
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle FROM contacts WHERE is_verified = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                p2p_pubkey: row.get(1)?,
                static_key,
                solana_address: row.get(3)?,
                global_name: row.get(4)?,
                local_alias: row.get(5)?,
                avatar_base64: row.get(6)?,
                is_anchor_capable: row.get::<_, i32>(7)? != 0,
                retention_seconds: row.get(8)?,
                handle: row.get(9)?,
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
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle FROM contacts WHERE is_verified = 1 AND is_anchor_capable = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key.copy_from_slice(&static_key_vec);
            Ok(crate::identity::SovereignIdentity {
                peer_id: row.get(0)?,
                p2p_pubkey: row.get(1)?,
                static_key,
                solana_address: row.get(3)?,
                global_name: row.get(4)?,
                local_alias: row.get(5)?,
                avatar_base64: row.get(6)?,
                is_anchor_capable: true,
                retention_seconds: row.get(8)?,
                handle: row.get(9)?,
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

    /// Retrieves ALL mailbox messages hosted by this node, for handover to other nodes.
    pub fn get_all_hosted_mailbox_messages(&self) -> Result<Vec<(String, String, Vec<u8>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT recipient_hash, sender_peer_id, encrypted_payload FROM mailbox_messages")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?))
        })?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }

    /// Retrieves and removes a limited number of pending messages for a specific peer.
    pub fn fetch_mailbox_payloads(&self, recipient: &libp2p::PeerId) -> Result<Vec<(String, Vec<u8>)>> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        
        let recipient_hash = Self::hash_peer_id(recipient);
        let mut messages = Vec::new();
        let mut row_ids = Vec::new();

        {
            // CRITICAL FIX: Limit to 4 messages to stay under 1MB libp2p limit (assuming ~250KB per chunk)
            let mut stmt = tx.prepare("SELECT rowid, sender_peer_id, encrypted_payload FROM mailbox_messages WHERE recipient_hash = ?1 ORDER BY rowid ASC LIMIT 4")?;
            let rows = stmt.query_map(params![recipient_hash], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?))
            })?;

            for row in rows {
                let (id, sender, payload) = row?;
                row_ids.push(id);
                messages.push((sender, payload));
            }
        }

        for id in row_ids {
            tx.execute("DELETE FROM mailbox_messages WHERE rowid = ?1", params![id])?;
        }
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

    /// Retrieves the local profile (User's name, handle, avatar, and privacy mode).
    pub fn get_profile(&self) -> Result<Option<(Option<String>, Option<String>, Option<String>, i32)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT name, handle, avatar_base64, privacy_mode FROM profile WHERE id = 1")?;
        let mut rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    /// Saves or updates the local profile.
    pub fn set_profile(&self, name: Option<&str>, handle: Option<&str>, avatar: Option<&str>, privacy_mode: i32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO profile (id, name, handle, avatar_base64, privacy_mode) VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET 
                name = COALESCE(excluded.name, name), 
                handle = COALESCE(excluded.handle, handle),
                avatar_base64 = COALESCE(excluded.avatar_base64, avatar_base64),
                privacy_mode = excluded.privacy_mode",
            params![name, handle, avatar, privacy_mode],
        )?;
        Ok(())
    }

    /// Persists a message to the database. Thread-safe and suitable for background threads.
    pub fn store_message(&self, peer_id: &str, content: &str, is_me: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO messages (peer_id, content, is_me, status) VALUES (?1, ?2, ?3, ?4)",
            params![peer_id, content, is_me as i32, if is_me { 0 } else { 1 }],
        )?;
        Ok(())
    }

    pub fn store_message_with_id(&self, peer_id: &str, msg_id: &str, content: &str, is_me: bool, reply_to: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO messages (peer_id, msg_id, content, is_me, status, reply_to_msg_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6) 
             ON CONFLICT(msg_id) DO UPDATE SET content = ?3, reply_to_msg_id = ?6",
            params![peer_id, msg_id, content, is_me as i32, if is_me { 0 } else { 1 }, reply_to],
        )?;
        Ok(())
    }

    pub fn update_message_status(&self, msg_id: &str, status: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE messages SET status = ?1 WHERE msg_id = ?2",
            params![status as i32, msg_id],
        )?;
        Ok(())
    }

    pub fn update_message_status_for_peer(&self, peer_id: &str, status: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE messages SET status = ?1 WHERE peer_id = ?2",
            params![status as i32, peer_id],
        )?;
        Ok(())
    }

    /// Retrieves all messages for a specific peer, ordered by timestamp.
    pub fn get_messages_for_peer(&self, peer_id: &str) -> Result<Vec<(String, String, bool, i32, Option<String>, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT content, timestamp, is_me, status, msg_id, reply_to_msg_id FROM messages WHERE peer_id = ?1 ORDER BY timestamp ASC")?;
        let rows = stmt.query_map(params![peer_id], |row| {
            Ok((row.get(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)? != 0, row.get::<_, i32>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<String>>(5)?))
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    /// Retrieves unread message counts for all contacts and groups.
    pub fn get_unread_counts(&self) -> Result<serde_json::Value> {
        let conn = self.conn.lock();
        let mut counts = serde_json::Map::new();

        // Direct messages
        let mut stmt = conn.prepare("SELECT peer_id, COUNT(*) FROM messages WHERE is_me = 0 AND status = 1 GROUP BY peer_id")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            if let Ok((id, count)) = row {
                counts.insert(id, serde_json::Value::Number(count.into()));
            }
        }

        // Group messages
        let mut stmt = conn.prepare("SELECT group_id, COUNT(*) FROM group_messages WHERE is_me = 0 AND status = 1 GROUP BY group_id")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            if let Ok((id, count)) = row {
                counts.insert(id, serde_json::Value::Number(count.into()));
            }
        }

        Ok(serde_json::Value::Object(counts))
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

    // --- Group Mesh Storage ---

    pub fn upsert_group(&self, group_id: &str, name: &str, description: &str, members_json: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO groups (group_id, name, description, members_json) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(group_id) DO UPDATE SET name = excluded.name, description = excluded.description, members_json = excluded.members_json",
            params![group_id, name, description, members_json],
        )?;
        Ok(())
    }

    pub fn update_group_members(&self, group_id: &str, members_json: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE groups SET members_json = ?1 WHERE group_id = ?2",
            params![members_json, group_id],
        )?;
        Ok(())
    }

    pub fn update_group_member_profile(&self, peer_id: &str, name: &str, avatar: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        
        let mut updates = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id, members_json FROM groups")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;

            for row in rows {
                if let Ok(data) = row {
                    updates.push(data);
                }
            }
        }

        for (group_id, members_json) in updates {
            let mut members: Vec<crate::network::GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
            let mut changed = false;
            for m in members.iter_mut() {
                if m.peer_id == peer_id {
                    if m.alias.as_deref() != Some(name) {
                        m.alias = Some(name.to_string());
                        changed = true;
                    }
                    if avatar.is_some() && m.avatar_base64.as_deref() != avatar {
                        m.avatar_base64 = avatar.map(|s| s.to_string());
                        changed = true;
                    }
                }
            }
            if changed {
                if let Ok(updated_json) = serde_json::to_string(&members) {
                    let _ = conn.execute("UPDATE groups SET members_json = ?1 WHERE group_id = ?2", params![updated_json, group_id]);
                }
            }
        }
        Ok(())
    }

    pub fn get_group(&self, group_id: &str) -> Result<Option<GroupMeshInfo>> {
        let row = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare("SELECT group_id, name, members_json, description FROM groups WHERE group_id = ?1")?;
            
            let mut rows = stmt.query_map(params![group_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
            })?;
            
            rows.next().transpose()?
        };
        
        if let Some((gid, name, members_json, description)) = row {
            let secret_vec = self.load_group_secret(&gid)?.unwrap_or_default();
            let mut secret = [0u8; 32];
            if secret_vec.len() == 32 { secret.copy_from_slice(&secret_vec); }
            
            Ok(Some(GroupMeshInfo {
                group_id: gid,
                name,
                members_json,
                secret,
                description,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_group_members(&self, group_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT members_json FROM groups WHERE group_id = ?1")?;
        let mut rows = stmt.query_map(params![group_id], |row| row.get::<_, String>(0))?;
        if let Some(row) = rows.next() { Ok(Some(row?)) } else { Ok(None) }
    }

    pub fn store_group_message(&self, group_id: &str, sender_id: &str, msg_id: &str, content: &str, is_me: bool, reply_to: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO group_messages (group_id, sender_id, msg_id, content, is_me, status, reply_to_msg_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(msg_id) DO UPDATE SET content = excluded.content, reply_to_msg_id = excluded.reply_to_msg_id",
            params![group_id, sender_id, msg_id, content, is_me as i32, if is_me { 0 } else { 1 }, reply_to],
        )?;
        Ok(())
    }

    pub fn update_group_message_status(&self, group_id: &str, status: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE group_messages SET status = ?1 WHERE group_id = ?2",
            params![status as i32, group_id],
        )?;
        Ok(())
    }

    pub fn update_group_message_status_by_id(&self, msg_id: &str, status: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE group_messages SET status = ?1 WHERE msg_id = ?2",
            params![status as i32, msg_id],
        )?;
        Ok(())
    }

    pub fn save_group_secret(&self, group_id: &str, secret: &[u8]) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO group_secrets (group_id, secret_blob) VALUES (?1, ?2)
             ON CONFLICT(group_id) DO UPDATE SET secret_blob = excluded.secret_blob",
            params![group_id, secret],
        )?;
        Ok(())
    }

    pub fn load_group_secret(&self, group_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT secret_blob FROM group_secrets WHERE group_id = ?1")?;
        let mut rows = stmt.query_map(params![group_id], |row| row.get::<_, Vec<u8>>(0))?;
        if let Some(row) = rows.next() { Ok(Some(row?)) } else { Ok(None) }
    }

    pub fn get_all_groups(&self) -> Result<Vec<(String, String, String, String, u32)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT group_id, name, members_json, description, retention_hours FROM groups")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?;
        let mut groups = Vec::new();
        for row in rows { groups.push(row?); }
        Ok(groups)
    }

    pub fn delete_group(&self, group_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        let rows = conn.execute("DELETE FROM groups WHERE group_id = ?1", params![group_id])?;
        println!("[Storage] Deleted group {}: {} rows", group_id, rows);
        let msg_rows = conn.execute("DELETE FROM group_messages WHERE group_id = ?1", params![group_id])?;
        println!("[Storage] Deleted messages for group {}: {} rows", group_id, msg_rows);
        conn.execute("DELETE FROM group_secrets WHERE group_id = ?1", params![group_id])?;
        conn.execute("DELETE FROM pending_group_invites WHERE group_id = ?1", params![group_id])?;
        
        // Add to tombstone list to prevent auto-re-joining
        let _ = conn.execute("INSERT OR IGNORE INTO deleted_groups (group_id) VALUES (?1)", params![group_id]);
        
        Ok(())
    }

    pub fn is_group_deleted(&self, group_id: &str) -> bool {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare("SELECT 1 FROM deleted_groups WHERE group_id = ?1") {
            Ok(s) => s,
            Err(_) => return false,
        };
        stmt.exists(params![group_id]).unwrap_or(false)
    }

    pub fn untombstone_group(&self, group_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM deleted_groups WHERE group_id = ?1", params![group_id])?;
        Ok(())
    }

    // --- Pending Group Invites ---

    pub fn store_pending_invite(&self, invite: &PendingGroupInvite) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO pending_group_invites (group_id, name, description, inviter_peer_id, group_secret_wrapped, members_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(group_id) DO UPDATE SET name = excluded.name, description = excluded.description,
                inviter_peer_id = excluded.inviter_peer_id, group_secret_wrapped = excluded.group_secret_wrapped,
                members_json = excluded.members_json",
            params![invite.group_id, invite.name, invite.description, invite.inviter_peer_id, invite.group_secret_wrapped, invite.members_json],
        )?;
        Ok(())
    }

    pub fn get_pending_invites(&self) -> Result<Vec<PendingGroupInvite>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT group_id, name, description, inviter_peer_id, group_secret_wrapped, members_json FROM pending_group_invites ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| Ok(PendingGroupInvite {
            group_id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            inviter_peer_id: row.get(3)?,
            group_secret_wrapped: row.get(4)?,
            members_json: row.get(5)?,
        }))?;
        let mut invites = Vec::new();
        for row in rows { invites.push(row?); }
        Ok(invites)
    }

    pub fn get_pending_invite(&self, group_id: &str) -> Result<Option<PendingGroupInvite>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT group_id, name, description, inviter_peer_id, group_secret_wrapped, members_json FROM pending_group_invites WHERE group_id = ?1"
        )?;
        let mut rows = stmt.query_map(params![group_id], |row| Ok(PendingGroupInvite {
            group_id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            inviter_peer_id: row.get(3)?,
            group_secret_wrapped: row.get(4)?,
            members_json: row.get(5)?,
        }))?;
        if let Some(row) = rows.next() { Ok(Some(row?)) } else { Ok(None) }
    }

    pub fn delete_pending_invite(&self, group_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM pending_group_invites WHERE group_id = ?1", params![group_id])?;
        Ok(())
    }

    // --- Handle Registry ---

    pub fn get_handle_claim(&self, handle: &str) -> Result<Option<(String, i64, String, bool)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, timestamp, signatures_json, verified FROM handle_registry WHERE handle = ?1")?;
        let mut rows = stmt.query_map(params![handle], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, i32>(3)? != 0))
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_handle_claim(&self, handle: &str, peer_id: &str, timestamp: i64, signatures_json: &str, verified: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO handle_registry (handle, peer_id, timestamp, signatures_json, verified) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(handle) DO UPDATE SET 
                peer_id = excluded.peer_id, 
                timestamp = excluded.timestamp, 
                signatures_json = excluded.signatures_json, 
                verified = excluded.verified",
            params![handle, peer_id, timestamp, signatures_json, verified as i32],
        )?;
        Ok(())
    }

    // --- Push Tokens (Background Proxy) ---

    pub fn save_push_token(&self, peer_id: &str, device_type: &str, token: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO push_tokens (peer_id, device_type, push_token, last_seen) VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(peer_id) DO UPDATE SET device_type = excluded.device_type, push_token = excluded.push_token, last_seen = CURRENT_TIMESTAMP",
            params![peer_id, device_type, token],
        )?;
        Ok(())
    }

    pub fn get_push_token(&self, peer_id: &str) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT device_type, push_token FROM push_tokens WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn get_group_messages(&self, group_id: &str) -> Result<Vec<(String, String, String, String, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT sender_id, msg_id, content, timestamp, reply_to_msg_id FROM group_messages WHERE group_id = ?1 ORDER BY timestamp ASC")?;
        let rows = stmt.query_map(params![group_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }

    // --- Mesh Torrent Storage ---

    /// Stores a file chunk in the mesh storage, enforcing a 1GB limit.
    pub fn store_mesh_chunk(&self, file_hash: &str, chunk_index: u32, data: &[u8]) -> Result<bool> {
        let conn = self.conn.lock();
        
        // 1. Check current usage
        let total_bytes: i64 = conn.query_row("SELECT SUM(LENGTH(data)) FROM mesh_chunks", [], |row| row.get(0)).unwrap_or(0);
        let gigabyte = 1024 * 1024 * 1024;
        
        if total_bytes + data.len() as i64 > gigabyte {
            return Ok(false); // Quota exceeded
        }

        // 2. Store chunk
        conn.execute(
            "INSERT INTO mesh_chunks (file_hash, chunk_index, data, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![file_hash, chunk_index, data, Utc::now().timestamp()],
        )?;
        Ok(true)
    }

    pub fn get_mesh_chunk(&self, file_hash: &str, chunk_index: u32) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT data FROM mesh_chunks WHERE file_hash = ?1 AND chunk_index = ?2")?;
        let mut rows = stmt.query_map(params![file_hash, chunk_index], |row| row.get::<_, Vec<u8>>(0))?;
        if let Some(row) = rows.next() { Ok(Some(row?)) } else { Ok(None) }
    }

    pub fn delete_mesh_chunks(&self, file_hash: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM mesh_chunks WHERE file_hash = ?1", params![file_hash])?;
        Ok(())
    }

    pub fn prune_old_mesh_chunks(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let seven_days_ago = Utc::now().timestamp() - (7 * 24 * 60 * 60);
        let count = conn.execute("DELETE FROM mesh_chunks WHERE timestamp < ?1", params![seven_days_ago])?;
        Ok(count)
    }

    pub fn get_mesh_storage_usage(&self) -> Result<i64> {
        let conn = self.conn.lock();
        let total_bytes: i64 = conn.query_row("SELECT SUM(LENGTH(data)) FROM mesh_chunks", [], |row| row.get(0)).unwrap_or(0);
        Ok(total_bytes)
    }

    // --- Intro-Claw Storage Methods ---

    /// Prune expired session_cache entries (older than max_age_secs)
    pub fn prune_expired_sessions(&self, max_age_secs: u64) -> Result<usize> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
        let deleted = conn.execute("DELETE FROM session_cache WHERE last_active < ?1", params![cutoff_str])?;
        Ok(deleted as usize)
    }

    /// Prune expired crypto_sessions (older than max_age_secs)
    pub fn prune_expired_crypto_sessions(&self, max_age_secs: u64) -> Result<usize> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
        let deleted = conn.execute("DELETE FROM crypto_sessions WHERE updated_at < ?1", params![cutoff_str])?;
        Ok(deleted as usize)
    }

    /// Run SQLCipher PRAGMA optimize for performance
    pub fn run_pragma_optimize(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch("PRAGMA optimize")?;
        Ok(())
    }

    /// Get list of file hashes in the drive
    pub fn get_active_drive_hashes(&self) -> Vec<String> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare("SELECT file_hash FROM drive_files") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut hashes = Vec::new();
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for row in rows.flatten() {
                hashes.push(row);
            }
        }
        hashes
    }

    /// Cleanup mesh chunks not associated with any drive file
    pub fn cleanup_orphaned_mesh_chunks(&self, active_hashes: &[String]) -> Result<usize> {
        let conn = self.conn.lock();
        if active_hashes.is_empty() {
            // If no drive files, don't delete anything (safety)
            return Ok(0);
        }
        let placeholders: Vec<String> = active_hashes.iter().map(|_| "?".to_string()).collect();
        let query = format!("DELETE FROM mesh_chunks WHERE file_hash NOT IN ({})", placeholders.join(","));
        let params: Vec<Box<dyn rusqlite::types::ToSql>> = active_hashes.iter()
            .map(|h| Box::new(h.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let deleted = conn.execute(&query, param_refs.as_slice())?;
        Ok(deleted as usize)
    }

    /// Get storage usage: (drive_bytes, mesh_bytes, total_disk_bytes)
    pub fn get_storage_usage(&self) -> (u64, u64, u64) {
        let conn = self.conn.lock();
        let drive_bytes: i64 = conn.query_row("SELECT COALESCE(SUM(total_size), 0) FROM drive_files", [], |row| row.get(0)).unwrap_or(0);
        let mesh_bytes: i64 = conn.query_row("SELECT COALESCE(SUM(LENGTH(data)), 0) FROM mesh_chunks", [], |row| row.get(0)).unwrap_or(0);
        let total_disk = std::fs::metadata("/").map(|m| m.len()).unwrap_or(0);
        (drive_bytes.max(0) as u64, mesh_bytes.max(0) as u64, total_disk)
    }

    // --- Introvert Drive ---

    pub fn upsert_drive_file(&self, filename: &str, file_hash: &str, mime_type: &str, size: i64, local_path: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO drive_files (filename, file_hash, mime_type, total_size, local_path) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(file_hash) DO UPDATE SET filename = excluded.filename, local_path = excluded.local_path",
            params![filename, file_hash, mime_type, size, local_path],
        )?;
        Ok(())
    }

    pub fn get_all_drive_files(&self) -> Result<Vec<DriveFileMetadata>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT filename, file_hash, mime_type, total_size, local_path, is_backed_up, timestamp FROM drive_files ORDER BY timestamp DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(DriveFileMetadata {
                filename: row.get(0)?,
                file_hash: row.get(1)?,
                mime_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                total_size: row.get(3)?,
                local_path: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                is_backed_up: row.get::<_, i32>(5)? != 0,
                timestamp: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })
        })?;
        let mut files = Vec::new();
        for row in rows { 
            if let Ok(file) = row {
                files.push(file); 
            }
        }
        Ok(files)
    }

    pub fn get_drive_file_by_hash(&self, file_hash: &str) -> Result<Option<DriveFileMetadata>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT filename, file_hash, mime_type, total_size, local_path, is_backed_up, timestamp FROM drive_files WHERE file_hash = ?1")?;
        let mut rows = stmt.query_map(params![file_hash], |row| {
            Ok(DriveFileMetadata {
                filename: row.get(0)?,
                file_hash: row.get(1)?,
                mime_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                total_size: row.get(3)?,
                local_path: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                is_backed_up: row.get::<_, i32>(5)? != 0,
                timestamp: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn delete_drive_file(&self, file_hash: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM drive_files WHERE file_hash = ?1", params![file_hash])?;
        Ok(())
    }

    pub fn update_drive_backup_status(&self, file_hash: &str, is_backed_up: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE drive_files SET is_backed_up = ?1 WHERE file_hash = ?2", params![if is_backed_up { 1 } else { 0 }, file_hash])?;
        Ok(())
    }

    pub fn update_contact_alias(&self, peer_id: &str, alias: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE contacts SET local_alias = ?1 WHERE peer_id = ?2",
            params![alias, peer_id],
        )?;
        Ok(())
    }

    // --- Message Reactions ---

    pub fn add_message_reaction(&self, msg_id: &str, sender_id: &str, emoji: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO message_reactions (msg_id, sender_id, emoji) VALUES (?1, ?2, ?3)
             ON CONFLICT(msg_id, sender_id) DO UPDATE SET emoji = excluded.emoji, timestamp = CURRENT_TIMESTAMP",
            params![msg_id, sender_id, emoji],
        )?;
        Ok(())
    }

    pub fn get_message_reactions(&self, msg_id: &str) -> Result<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT sender_id, emoji FROM message_reactions WHERE msg_id = ?1")?;
        let rows = stmt.query_map(params![msg_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut reactions = Vec::new();
        for row in rows {
            let (sender, emoji) = row?;
            reactions.push(serde_json::json!({ "sender_id": sender, "emoji": emoji }));
        }
        Ok(serde_json::json!(reactions))
    }

    // --- Chat History & Auto-Erase Management ---

    pub fn set_contact_retention(&self, peer_id: &str, hours: u32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE contacts SET retention_hours = ?1 WHERE peer_id = ?2", params![hours, peer_id])?;
        Ok(())
    }

    pub fn get_contact_retention(&self, peer_id: &str) -> Result<u32> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT retention_hours FROM contacts WHERE peer_id = ?1")?;
        let hours: u32 = stmt.query_row(params![peer_id], |row| row.get(0)).unwrap_or(0);
        Ok(hours)
    }

    pub fn set_group_retention(&self, group_id: &str, hours: u32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE groups SET retention_hours = ?1 WHERE group_id = ?2", params![hours, group_id])?;
        Ok(())
    }

    pub fn get_group_retention(&self, group_id: &str) -> Result<u32> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT retention_hours FROM groups WHERE group_id = ?1")?;
        let hours: u32 = stmt.query_row(params![group_id], |row| row.get(0)).unwrap_or(0);
        Ok(hours)
    }

    pub fn delete_message(&self, msg_id: &str, is_group: bool, deleted_by_admin: bool) -> Result<()> {
        let conn = self.conn.lock();
        if is_group {
            if deleted_by_admin {
                conn.execute("UPDATE group_messages SET content = '[DELETED_BY_ADMIN]' WHERE msg_id = ?1", params![msg_id])?;
            } else {
                conn.execute("DELETE FROM group_messages WHERE msg_id = ?1", params![msg_id])?;
            }
        } else {
            conn.execute("DELETE FROM messages WHERE msg_id = ?1", params![msg_id])?;
        }
        if !deleted_by_admin {
            conn.execute("DELETE FROM message_reactions WHERE msg_id = ?1", params![msg_id])?;
        }
        Ok(())
    }

    pub fn edit_message(&self, msg_id: &str, new_content: &str, is_group: bool) -> Result<()> {
        let conn = self.conn.lock();
        if is_group {
            conn.execute("UPDATE group_messages SET content = ?1 WHERE msg_id = ?2", params![new_content, msg_id])?;
        } else {
            conn.execute("UPDATE messages SET content = ?1 WHERE msg_id = ?2", params![new_content, msg_id])?;
        }
        Ok(())
    }

    pub fn update_group_muted_members(&self, group_id: &str, muted_json: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE groups SET muted_members_json = ?1 WHERE group_id = ?2",
            params![muted_json, group_id],
        )?;
        Ok(())
    }

    pub fn get_group_muted_members(&self, group_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT muted_members_json FROM groups WHERE group_id = ?1")?;
        let json: String = stmt.query_row(params![group_id], |row| row.get(0)).unwrap_or_else(|_| "[]".to_string());
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    pub fn prune_expired_messages(&self) -> Result<()> {
        let conn = self.conn.lock();

        // Prune 1-on-1 messages
        let mut stmt_contacts = conn.prepare("SELECT peer_id, retention_hours FROM contacts WHERE retention_hours > 0")?;
        let contacts_iter = stmt_contacts.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)))?;
        for contact in contacts_iter {
            if let Ok((peer_id, hours)) = contact {
                conn.execute(
                    "DELETE FROM messages WHERE peer_id = ?1 AND timestamp <= datetime('now', '-' || ?2 || ' hours')",
                    params![peer_id, hours.to_string()],
                )?;
            }
        }

        // Prune group messages
        let mut stmt_groups = conn.prepare("SELECT group_id, retention_hours FROM groups WHERE retention_hours > 0")?;
        let groups_iter = stmt_groups.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)))?;
        for group in groups_iter {
            if let Ok((group_id, hours)) = group {
                conn.execute(
                    "DELETE FROM group_messages WHERE group_id = ?1 AND timestamp <= datetime('now', '-' || ?2 || ' hours')",
                    params![group_id, hours.to_string()],
                )?;
            }
        }
        Ok(())
    }

    // ==================== NOTES ====================

    pub fn create_note(&self, id: &str, title: &str, content: &str, tags: &str, image_path: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO notes (id, title, content, tags, image_path) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, content, tags, image_path],
        )?;
        Ok(())
    }

    pub fn update_note(&self, id: &str, title: &str, content: &str, tags: &str, image_path: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE notes SET title = ?2, content = ?3, tags = ?4, image_path = ?5, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, title, content, tags, image_path],
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM note_versions WHERE note_id = ?1", params![id])?;
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_note(&self, id: &str) -> Result<Option<(String, String, String, String, Option<String>, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, title, content, tags, image_path, created_at, updated_at FROM notes WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_notes(&self) -> Result<Vec<(String, String, String, String, Option<String>, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, title, content, tags, image_path, created_at, updated_at FROM notes ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows { notes.push(row?); }
        Ok(notes)
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<(String, String, String, String, Option<String>, String, String)>> {
        let conn = self.conn.lock();
        let search_pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, title, content, tags, image_path, created_at, updated_at FROM notes 
             WHERE title LIKE ?1 OR content LIKE ?1 OR tags LIKE ?1 
             ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map(params![search_pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows { notes.push(row?); }
        Ok(notes)
    }

    pub fn save_note_version(&self, note_id: &str, title: &str, content: &str, tags: &str) -> Result<i32> {
        let conn = self.conn.lock();
        let version: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM note_versions WHERE note_id = ?1",
            params![note_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO note_versions (note_id, title, content, tags, version_number) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![note_id, title, content, tags, version],
        )?;
        Ok(version)
    }

    pub fn get_note_versions(&self, note_id: &str) -> Result<Vec<(i32, String, String, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT version_number, title, content, tags, created_at FROM note_versions WHERE note_id = ?1 ORDER BY version_number DESC"
        )?;
        let rows = stmt.query_map(params![note_id], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut versions = Vec::new();
        for row in rows { versions.push(row?); }
        Ok(versions)
    }

    // ==================== CALL HISTORY ====================

    pub fn log_call(&self, peer_id: &str, call_type: &str, media_type: i32, duration_seconds: i32, is_incoming: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO call_history (peer_id, call_type, media_type, duration_seconds, is_incoming) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![peer_id, call_type, media_type, duration_seconds, is_incoming as i32],
        )?;
        Ok(())
    }

    pub fn get_call_history(&self, limit: i32) -> Result<Vec<(String, String, i32, i32, bool, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT peer_id, call_type, media_type, duration_seconds, is_incoming, timestamp FROM call_history ORDER BY timestamp DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)? == 1,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut history = Vec::new();
        for row in rows { history.push(row?); }
        Ok(history)
    }

    pub fn get_call_count(&self) -> Result<i32> {
        let conn = self.conn.lock();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM call_history", [], |row| row.get(0))?;
        Ok(count)
    }

    // ==================== LAST SEEN ====================

    pub fn update_last_seen(&self, peer_id: &str, timestamp: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE contacts SET last_seen = ?1 WHERE peer_id = ?2",
            params![timestamp, peer_id],
        )?;
        Ok(())
    }

    pub fn get_last_seen(&self, peer_id: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock();
        let result: Option<i64> = conn.query_row(
            "SELECT last_seen FROM contacts WHERE peer_id = ?1",
            params![peer_id],
            |row| row.get(0),
        )?;
        Ok(result)
    }

    // ==================== MESSAGE SEARCH ====================

    pub fn search_messages(&self, peer_id: &str, query: &str) -> Result<Vec<(String, String, bool, i32, Option<String>, Option<String>)>> {
        let conn = self.conn.lock();
        let search_pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT content, timestamp, is_me, status, msg_id, reply_to_msg_id FROM messages WHERE peer_id = ?1 AND content LIKE ?2 ORDER BY timestamp ASC"
        )?;
        let rows = stmt.query_map(params![peer_id, search_pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)? != 0,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }

    pub fn search_group_messages(&self, group_id: &str, query: &str) -> Result<Vec<(String, String, String, String, Option<String>)>> {
        let conn = self.conn.lock();
        let search_pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT sender_id, msg_id, content, timestamp, reply_to_msg_id FROM group_messages WHERE group_id = ?1 AND content LIKE ?2 ORDER BY timestamp ASC"
        )?;
        let rows = stmt.query_map(params![group_id, search_pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }
}
