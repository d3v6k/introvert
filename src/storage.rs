use rusqlite::{params, Connection};
use anyhow::Result;
use std::path::Path;
use std::collections::HashMap;
use std::time::{Instant, Duration, SystemTime};
use parking_lot::Mutex;
use sha2::{Sha256, Digest};
use chrono::Utc;
use tracing::{info, warn};
use crate::identity::SovereignIdentity;

/// Cached value with expiration
struct CachedValue<T: Clone> {
    value: T,
    inserted_at: Instant,
    ttl: Duration,
}

impl<T: Clone> CachedValue<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self { value, inserted_at: Instant::now(), ttl }
    }

    fn is_valid(&self) -> bool {
        self.inserted_at.elapsed() < self.ttl
    }

    fn get(&self) -> Option<&T> {
        if self.is_valid() { Some(&self.value) } else { None }
    }
}

pub struct StorageService {
    conn: Mutex<Connection>,
    _is_ephemeral: bool,
    // In-memory caches for frequently accessed data
    profile_cache: Mutex<Option<CachedValue<(Option<String>, Option<String>, Option<String>, i32, i32)>>>,
    contacts_cache: Mutex<Option<CachedValue<Vec<SovereignIdentity>>>>,
    storage_usage_cache: Mutex<Option<CachedValue<(u64, u64, u64)>>>,
    anchor_nodes_cache: Mutex<Option<CachedValue<Vec<SovereignIdentity>>>>,
}

/// Cache TTL constants
const PROFILE_CACHE_TTL: Duration = Duration::from_secs(30); // 30 seconds
const CONTACTS_CACHE_TTL: Duration = Duration::from_secs(60); // 1 minute
const STORAGE_USAGE_CACHE_TTL: Duration = Duration::from_secs(15); // 15 seconds
const ANCHOR_NODES_CACHE_TTL: Duration = Duration::from_secs(120); // 2 minutes

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
    /// Escapes LIKE special characters to prevent unintended pattern matches.
    fn escape_like(input: &str) -> String {
        input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    }

    /// Safely copies a byte slice to a 32-byte array, returning zeros if length mismatches.
    fn safe_static_key(bytes: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        let copy_len = bytes.len().min(32);
        key[..copy_len].copy_from_slice(&bytes[..copy_len]);
        key
    }

    /// Creates a new SQLCipher encrypted database at the given path.
    ///
    /// The 256-bit key must be derived via HKDF-SHA256 from the master seed
    /// using the `b"introvert_storage_key"` domain-separation salt. This
    /// function never writes the key to disk — it lives exclusively in
    /// volatile RAM for the duration of the connection setup.
    pub fn new<P: AsRef<Path> + std::fmt::Display>(path: P, key: &[u8; 32]) -> Result<Self> {
        let path_display = format!("{}", path);
        let conn = Connection::open(&path)?;

        // Initialize SQLCipher encryption — key must be set first.
        // Cipher PRAGMAs (page_size, kdf_iter, hmac, kdf_algorithm) are NOT
        // set here because they must match the database's original creation
        // settings. Changing them on an existing database causes SQLITE_NOTADB.
        let key_hex = hex::encode(key);
        conn.pragma_update(None, "key", format!("x'{}'", key_hex))?;

        // Corruption detection: if the key is wrong or the header is damaged,
        // the first real SQL operation returns SQLITE_NOTADB or SQLITE_CORRUPT.
        match conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0)) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ffi::ErrorCode::NotADatabase
                    || e.code == rusqlite::ffi::ErrorCode::DatabaseCorrupt =>
            {
                warn!("[Storage] Database decryption failed at '{}'", path_display);
                return Err(anyhow::anyhow!(
                    "STORAGE_DECRYPT_FAILED: Cannot decrypt database at '{}'. \
                     The encryption key may be incorrect or the file is corrupt.",
                    path_display
                ));
            }
            Err(e) => {
                warn!("[Storage] Unexpected error during decryption probe: {}", e);
                return Err(anyhow::anyhow!("STORAGE_INIT_FAILED: {}", e));
            }
        }

        // WAL mode: crash-safe, better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")?;

        let slf = Self {
            conn: Mutex::new(conn),
            _is_ephemeral: false,
            profile_cache: Mutex::new(None),
            contacts_cache: Mutex::new(None),
            storage_usage_cache: Mutex::new(None),
            anchor_nodes_cache: Mutex::new(None),
        };
        slf.bootstrap()?;
        Ok(slf)
    }

    /// Creates a memory-only non-encrypted database for stress testing.
    pub fn new_ephemeral() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let slf = Self {
            conn: Mutex::new(conn),
            _is_ephemeral: true,
            profile_cache: Mutex::new(None),
            contacts_cache: Mutex::new(None),
            storage_usage_cache: Mutex::new(None),
            anchor_nodes_cache: Mutex::new(None),
        };
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
            CREATE TABLE IF NOT EXISTS pending_group_invites (
                group_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                inviter_peer_id TEXT NOT NULL,
                group_secret_wrapped BLOB NOT NULL,
                members_json TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS _schema_version (version INTEGER PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS dead_letter_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                peer_id TEXT NOT NULL,
                payload BLOB NOT NULL,
                queued_at INTEGER NOT NULL,
                retry_count INTEGER DEFAULT 0,
                last_retry_at INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_dlq_peer ON dead_letter_queue (peer_id);
            CREATE INDEX IF NOT EXISTS idx_dlq_queued ON dead_letter_queue (queued_at);"
        )?;

        // Pending file chunks — persistent queue for cross-network file transfers
        // Used when no RBNs are connected and chunks would otherwise be lost on app restart
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_file_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                transfer_id TEXT NOT NULL,
                peer_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                chunk_data BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                retry_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(transfer_id, chunk_index)
            );
            CREATE INDEX IF NOT EXISTS idx_pending_chunks_peer ON pending_file_chunks(peer_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_pending_chunks_transfer ON pending_file_chunks(transfer_id, chunk_index);"
        )?;

        // Cleared chats — tracks when a chat was cleared to prevent mailbox re-delivery of old messages
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cleared_chats (
                peer_id TEXT PRIMARY KEY,
                cleared_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );"
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

        // Elevated messages (persistent bookmarks, per-chat)
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS elevated_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                chat_id TEXT NOT NULL,
                msg_id TEXT NOT NULL,
                content TEXT NOT NULL,
                sender_id TEXT,
                is_me INTEGER DEFAULT 0,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                elevated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(chat_id, msg_id)
            )", []
        )?;
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_elevated_chat ON elevated_messages (chat_id, elevated_at DESC)", []);

        // Daily rewards system — consolidated reward records with anti-farming tracking
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS daily_reward_records (
                cycle_date TEXT PRIMARY KEY,
                total_social_points REAL NOT NULL DEFAULT 0.0,
                total_infra_points REAL NOT NULL DEFAULT 0.0,
                active_containers_highwater INTEGER NOT NULL DEFAULT 0,
                total_cycle_uptime_secs INTEGER NOT NULL DEFAULT 0
            )", []
        )?;

        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS daily_cycles (
                cycle_date TEXT PRIMARY KEY,
                snapshot_balance INTEGER NOT NULL DEFAULT 0,
                total_points REAL NOT NULL DEFAULT 0.0,
                capped_points REAL NOT NULL DEFAULT 0.0,
                intr_reward REAL NOT NULL DEFAULT 0.0,
                unique_peers INTEGER NOT NULL DEFAULT 0,
                is_eligible INTEGER NOT NULL DEFAULT 0,
                eligibility_reason TEXT NOT NULL DEFAULT '',
                submitted INTEGER NOT NULL DEFAULT 0,
                started_at INTEGER NOT NULL,
                ended_at INTEGER
            )", []
        )?;
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS daily_activity_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                cycle_date TEXT NOT NULL,
                activity_type INTEGER NOT NULL,
                raw_count INTEGER NOT NULL DEFAULT 0,
                capped_count INTEGER NOT NULL DEFAULT 0,
                points REAL NOT NULL DEFAULT 0.0,
                UNIQUE(cycle_date, activity_type)
            )", []
        )?;
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS daily_reward_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                weights_json TEXT NOT NULL,
                anti_gaming_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )", []
        )?;

        // Per-node daily allocation results from the 10-year emission decay ledger
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS node_daily_allocations (
                cycle_date TEXT NOT NULL,
                peer_id TEXT NOT NULL,
                sol_address TEXT NOT NULL DEFAULT '',
                raw_points REAL NOT NULL DEFAULT 0.0,
                weighted_points REAL NOT NULL DEFAULT 0.0,
                share_of_pool REAL NOT NULL DEFAULT 0.0,
                intr_allocated REAL NOT NULL DEFAULT 0.0,
                tier_multiplier REAL NOT NULL DEFAULT 1.0,
                created_at INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (cycle_date, peer_id)
            )", []
        )?;

        // Migrations: All ALTER TABLE ADD COLUMN failures are intentionally discarded
        // because they succeed on first run and fail with "duplicate column" on subsequent runs.
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
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN prestige_tier INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE contacts ADD COLUMN last_seen INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE profile ADD COLUMN prestige_tier INTEGER DEFAULT 0", []);
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS pending_group_invites (
                group_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                inviter_peer_id TEXT NOT NULL,
                group_secret_wrapped BLOB NOT NULL,
                members_json TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        );

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

    /// Gets the Intro-Claw active state (true = engine running, false = engine stopped)
    pub fn get_intro_claw_active(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'intro_claw_active'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val == "true";
                }
            }
        }
        false // Default: engine not active
    }

    /// Sets the Intro-Claw active state
    pub fn set_intro_claw_active(&self, active: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('intro_claw_active', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![active.to_string()],
        )?;
        Ok(())
    }

    /// Gets the Intro-Claw node mode state
    pub fn get_intro_claw_node_mode(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'intro_claw_node_mode'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val == "true";
                }
            }
        }
        false
    }

    /// Sets the Intro-Claw node mode state
    pub fn set_intro_claw_node_mode(&self, enabled: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('intro_claw_node_mode', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![enabled.to_string()],
        )?;
        Ok(())
    }

    pub fn get_intro_claw_endpoint(&self) -> String {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'intro_claw_endpoint'") {
            if let Ok(mut rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                if let Some(Ok(val)) = rows.next() {
                    return val;
                }
            }
        }
        String::new()
    }

    pub fn set_intro_claw_endpoint(&self, endpoint: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('intro_claw_endpoint', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![endpoint],
        )?;
        Ok(())
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

    /// Sets the Terms of Use disclaimer acceptance status and version.
    pub fn set_disclaimer_accepted(&self, accepted: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('disclaimer_accepted', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![accepted.to_string()],
        )?;
        conn.execute(
            "INSERT INTO economy_meta (key, value) VALUES ('disclaimer_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params!["1.0"],
        )?;
        Ok(())
    }

    /// Checks if the Terms of Use disclaimer has been accepted.
    pub fn is_disclaimer_accepted(&self) -> bool {
        let conn = self.conn.lock();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM economy_meta WHERE key = 'disclaimer_accepted'") {
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
            "INSERT INTO contacts (peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_verified, is_incoming, is_anchor_capable, handle, prestige_tier) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) 
             ON CONFLICT(peer_id) DO UPDATE SET 
                p2p_pubkey = excluded.p2p_pubkey,
                static_key = excluded.static_key, 
                solana_address = excluded.solana_address, 
                global_name = excluded.global_name,
                local_alias = COALESCE(contacts.local_alias, excluded.local_alias),
                avatar_base64 = excluded.avatar_base64,
                is_verified = excluded.is_verified,
                is_incoming = excluded.is_incoming,
                is_anchor_capable = excluded.is_anchor_capable,
                handle = excluded.handle,
                prestige_tier = excluded.prestige_tier",
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
                identity.handle,
                identity.prestige_tier.unwrap_or(0) as i32
            ],
        )?;
        // Invalidate contacts cache
        let mut cache = self.contacts_cache.lock();
        *cache = None;
        let mut anchor_cache = self.anchor_nodes_cache.lock();
        *anchor_cache = None;
        Ok(())
    }

    pub fn update_contact_verification(&self, peer_id: &str, is_verified: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE contacts SET is_verified = ?1 WHERE peer_id = ?2", params![if is_verified { 1 } else { 0 }, peer_id])?;
        // Invalidate contacts cache so get_all_contacts() picks up the change
        let mut cache = self.contacts_cache.lock();
        *cache = None;
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
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle, prestige_tier FROM contacts WHERE peer_id = ?1")?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key = Self::safe_static_key(&static_key_vec);
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
                prestige_tier: row.get::<_, Option<i32>>(10)?.map(|v| v as u8),
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
        drop(conn);
        // Invalidate contacts cache so get_all_contacts() reflects the deletion
        let mut cache = self.contacts_cache.lock();
        *cache = None;
        let mut anchor_cache = self.anchor_nodes_cache.lock();
        *anchor_cache = None;
        Ok(())
    }

    /// Removes all messages for a specific peer (Deletes chat history) and clears local mailbox entries.
    pub fn delete_chat(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE peer_id = ?1", params![peer_id])?;
        // Also clear local mailbox entries for this peer to prevent re-delivery
        if let Ok(parsed) = peer_id.parse::<libp2p::PeerId>() {
            let recipient_hash = Self::hash_peer_id(&parsed);
            conn.execute("DELETE FROM mailbox_messages WHERE recipient_hash = ?1", params![recipient_hash])?;
        }
        // Record the clear timestamp to prevent mailbox drain from re-delivering old messages
        conn.execute(
            "INSERT OR REPLACE INTO cleared_chats (peer_id, cleared_at) VALUES (?1, CURRENT_TIMESTAMP)",
            params![peer_id],
        )?;
        Ok(())
    }

    /// Check if a message should be skipped because the chat was cleared after it was sent.
    pub fn should_skip_mailbox_message(&self, sender_peer_id: &str, msg_timestamp: i64) -> bool {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT cleared_at FROM cleared_chats WHERE peer_id = ?1",
            params![sender_peer_id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(cleared_at_str) => {
                // Parse the cleared_at timestamp and compare with message timestamp
                if let Ok(cleared_dt) = chrono::NaiveDateTime::parse_from_str(&cleared_at_str, "%Y-%m-%d %H:%M:%S") {
                    let cleared_ts = cleared_dt.and_utc().timestamp();
                    msg_timestamp < cleared_ts
                } else {
                    false
                }
            }
            Err(_) => false, // No clear record, don't skip
        }
    }

    /// Clean up old cleared_chats entries (older than 7 days).
    pub fn cleanup_cleared_chats(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM cleared_chats WHERE cleared_at < datetime('now', '-7 days')",
            [],
        )?;
        Ok(())
    }

    /// Clean up old reward_log telemetry entries (older than 7 days) to prevent database bloat.
    pub fn cleanup_expired_reward_logs(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM reward_log WHERE timestamp < datetime('now', '-7 days')",
            [],
        )?;
        Ok(())
    }

    /// Removes all sovereign contacts.
    pub fn clear_all_contacts(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM contacts", [])?;
        conn.execute("DELETE FROM session_cache", [])?;
        conn.execute("DELETE FROM mailbox_messages", [])?;
        drop(conn);
        // Invalidate contacts cache so get_all_contacts() reflects the deletion
        let mut cache = self.contacts_cache.lock();
        *cache = None;
        let mut anchor_cache = self.anchor_nodes_cache.lock();
        *anchor_cache = None;
        Ok(())
    }

    /// Retrieves all verified sovereign contacts.
    pub fn get_all_contacts(&self) -> Result<Vec<crate::identity::SovereignIdentity>> {
        // Check cache first
        {
            let cache = self.contacts_cache.lock();
            if let Some(cached) = &*cache {
                if let Some(value) = cached.get() {
                    return Ok(value.clone());
                }
            }
        }

        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle, prestige_tier FROM contacts WHERE is_verified = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key = Self::safe_static_key(&static_key_vec);
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
                prestige_tier: row.get::<_, Option<i32>>(10)?.map(|v| v as u8),
            })
        })?;

        let mut contacts = Vec::new();
        for row in rows {
            contacts.push(row?);
        }
        
        // Update cache
        let mut cache = self.contacts_cache.lock();
        *cache = Some(CachedValue::new(contacts.clone(), CONTACTS_CACHE_TTL));
        
        Ok(contacts)
    }

    /// Fetches all verified contacts marked as Anchor Capable.
    pub fn fetch_all_anchor_nodes(&self) -> Result<Vec<crate::identity::SovereignIdentity>> {
        // Check cache first
        {
            let cache = self.anchor_nodes_cache.lock();
            if let Some(cached) = &*cache {
                if let Some(value) = cached.get() {
                    return Ok(value.clone());
                }
            }
        }

        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, retention_hours, handle, prestige_tier FROM contacts WHERE is_verified = 1 AND is_anchor_capable = 1")?;
        let rows = stmt.query_map([], |row| {
            let static_key_vec: Vec<u8> = row.get(2)?;
            let mut static_key = [0u8; 32];
            static_key = Self::safe_static_key(&static_key_vec);
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
                prestige_tier: row.get::<_, Option<i32>>(10)?.map(|v| v as u8),
            })
        })?;

        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row?);
        }
        
        // Update cache
        let mut cache = self.anchor_nodes_cache.lock();
        *cache = Some(CachedValue::new(nodes.clone(), ANCHOR_NODES_CACHE_TTL));
        
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
    pub fn get_profile(&self) -> Result<Option<(Option<String>, Option<String>, Option<String>, i32, i32)>> {
        // Check cache first
        {
            let cache = self.profile_cache.lock();
            if let Some(cached) = &*cache {
                if let Some(value) = cached.get() {
                    return Ok(Some(value.clone()));
                }
            }
        }

        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT name, handle, avatar_base64, privacy_mode, prestige_tier FROM profile WHERE id = 1")?;
        let mut rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?;

        if let Some(row) = rows.next() {
            let value = row?;
            // Update cache
            let mut cache = self.profile_cache.lock();
            *cache = Some(CachedValue::new(value.clone(), PROFILE_CACHE_TTL));
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Saves or updates the local profile.
    pub fn set_profile(&self, name: Option<&str>, handle: Option<&str>, avatar: Option<&str>, privacy_mode: i32) -> Result<()> {
        // IMMUTABILITY GUARD: If the existing handle is permanently verified in handle_registry,
        // reject any attempt to change it to a different value.
        if let Ok(Some((_, existing_handle, _, _, _))) = self.get_profile() {
            if let Some(ref h) = existing_handle {
                if !h.is_empty() {
                    if let Ok(claimed) = self.is_handle_permanently_claimed(h) {
                        if claimed {
                            // Only allow the update if the handle is unchanged (or null, meaning "don't update")
                            let handle_unchanged = match handle {
                                Some(new_h) => new_h == h,
                                None => true, // null means "keep existing"
                            };
                            if !handle_unchanged {
                                tracing::warn!("[Storage] Rejecting handle change: '{}' is permanently claimed. Cannot change to '{}'.", h, handle.unwrap_or("null"));
                                anyhow::bail!("Handle '{}' is permanently claimed and cannot be changed", h);
                            }
                        }
                    }
                }
            }
        }

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
        // Invalidate profile cache
        let mut cache = self.profile_cache.lock();
        *cache = None;
        Ok(())
    }

    /// Updates the local profile's prestige tier (called from Dart when INTR balance changes).
    pub fn set_profile_tier(&self, tier: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO profile (id, prestige_tier) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET prestige_tier = ?1",
            params![tier as i32],
        )?;
        Ok(())
    }

    /// Updates a contact's prestige tier (received via ProfileResponse).
    pub fn set_contact_tier(&self, peer_id: &str, tier: u8) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE contacts SET prestige_tier = ?1 WHERE peer_id = ?2",
            params![tier as i32, peer_id],
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

    pub fn store_message_with_id(&self, peer_id: &str, msg_id: &str, content: &str, is_me: bool, reply_to: Option<&str>, timestamp: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO messages (peer_id, msg_id, content, is_me, status, reply_to_msg_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, CURRENT_TIMESTAMP)) 
             ON CONFLICT(msg_id) DO UPDATE SET content = ?3, reply_to_msg_id = ?6, timestamp = COALESCE(?7, timestamp)",
            params![peer_id, msg_id, content, is_me as i32, if is_me { 0 } else { 1 }, reply_to, timestamp],
        )?;
        Ok(())
    }

    /// Sync-safe insert: only adds new messages, never overwrites existing ones.
    /// Used by ChatSyncResponse to prevent stale sync data from overwriting current messages.
    pub fn store_message_if_new(&self, peer_id: &str, msg_id: &str, content: &str, is_me: bool, reply_to: Option<&str>, timestamp: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO messages (peer_id, msg_id, content, is_me, status, reply_to_msg_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, CURRENT_TIMESTAMP))",
            params![peer_id, msg_id, content, is_me as i32, if is_me { 0 } else { 1 }, reply_to, timestamp],
        )?;
        Ok(())
    }

    /// Check if a message already exists by msg_id (for dedup).
    pub fn message_exists(&self, msg_id: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE msg_id = ?1",
            params![msg_id],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(0) > 0
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

    pub fn update_message_status_if_higher(&self, msg_id: &str, new_status: u8) -> Result<bool> {
        let conn = self.conn.lock();
        let current: i32 = conn.query_row(
            "SELECT COALESCE(status, 0) FROM messages WHERE msg_id = ?1",
            params![msg_id],
            |row| row.get(0),
        ).unwrap_or(0);
        let current = current as u8;
        // Allowed transitions: 0→3, 0→1, 0→2, 3→1, 3→2, 1→2
        let allowed = match current {
            0 => new_status <= 3,
            3 => new_status == 1 || new_status == 2,
            1 => new_status == 2,
            _ => false,
        };
        if allowed && new_status > current {
            conn.execute("UPDATE messages SET status = ?1 WHERE msg_id = ?2 AND status = ?3",
                params![new_status as i32, msg_id, current as i32])?;
            return Ok(true);
        }
        Ok(false)
    }

    // --- Pending file chunk queue (persistent safety net for cross-network transfers) ---

    pub fn enqueue_pending_chunk(&self, transfer_id: &str, peer_id: &str, chunk_index: u32, chunk_data: &[u8]) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO pending_file_chunks (transfer_id, peer_id, chunk_index, chunk_data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![transfer_id, peer_id, chunk_index as i32, chunk_data, chrono::Utc::now().timestamp()],
        )?;
        Ok(())
    }

    pub fn dequeue_pending_chunks(&self, peer_id: &str, limit: usize) -> Result<Vec<(String, u32, Vec<u8>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT transfer_id, chunk_index, chunk_data FROM pending_file_chunks WHERE peer_id = ?1 ORDER BY transfer_id ASC, chunk_index ASC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![peer_id, limit as i32], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)? as u32, row.get::<_, Vec<u8>>(2)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn remove_pending_chunk(&self, transfer_id: &str, chunk_index: u32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM pending_file_chunks WHERE transfer_id = ?1 AND chunk_index = ?2",
            params![transfer_id, chunk_index as i32],
        )?;
        Ok(())
    }

    /// Increment retry count for a pending chunk. Returns the new count.
    /// If max_retries is exceeded, deletes the chunk to prevent infinite retry loops.
    pub fn increment_chunk_retry(&self, transfer_id: &str, chunk_index: u32, max_retries: i32) -> Result<i32> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE pending_file_chunks SET retry_count = retry_count + 1 WHERE transfer_id = ?1 AND chunk_index = ?2",
            params![transfer_id, chunk_index as i32],
        )?;
        let new_count: i32 = conn.query_row(
            "SELECT retry_count FROM pending_file_chunks WHERE transfer_id = ?1 AND chunk_index = ?2",
            params![transfer_id, chunk_index as i32],
            |row| row.get(0),
        ).unwrap_or(0);
        if new_count >= max_retries {
            conn.execute(
                "DELETE FROM pending_file_chunks WHERE transfer_id = ?1 AND chunk_index = ?2",
                params![transfer_id, chunk_index as i32],
            )?;
        }
        Ok(new_count)
    }

    pub fn remove_pending_chunks_for_transfer(&self, transfer_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM pending_file_chunks WHERE transfer_id = ?1",
            params![transfer_id],
        )?;
        Ok(())
    }

    pub fn cleanup_stale_pending_chunks(&self, max_age_secs: i64) -> Result<usize> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now().timestamp() - max_age_secs;
        let deleted = conn.execute(
            "DELETE FROM pending_file_chunks WHERE created_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    pub fn has_pending_chunks_for_peer(&self, peer_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM pending_file_chunks WHERE peer_id = ?1",
            params![peer_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Fetch sent messages stuck at status=0 (Sent) older than `age_secs` seconds.
    /// Returns (msg_id, peer_id, content, reply_to) for retry.
    pub fn fetch_undelivered_messages(&self, age_secs: i64) -> Result<Vec<(String, String, String, Option<String>)>> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now().timestamp() - age_secs;
        let mut stmt = conn.prepare(
            "SELECT msg_id, peer_id, content, reply_to_msg_id FROM messages \
             WHERE is_me = 1 AND status = 0 AND msg_id IS NOT NULL \
             AND CAST(strftime('%s', timestamp) AS INTEGER) < ?1"
        )?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
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

    /// Retrieves messages for a peer with pagination (most recent first).
    pub fn get_messages_for_peer_paginated(&self, peer_id: &str, offset: u32, limit: u32) -> Result<Vec<(String, String, bool, i32, Option<String>, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT content, timestamp, is_me, status, msg_id, reply_to_msg_id FROM messages WHERE peer_id = ?1 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3")?;
        let rows = stmt.query_map(params![peer_id, limit, offset], |row| {
            Ok((row.get(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)? != 0, row.get::<_, i32>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<String>>(5)?))
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        messages.reverse(); // Return in chronological order
        Ok(messages)
    }

    /// Retrieves only the last message for a specific peer (optimized for chat list preview).
    pub fn get_last_message_for_peer(&self, peer_id: &str) -> Result<Option<(String, String, bool, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT content, timestamp, is_me, msg_id FROM messages WHERE peer_id = ?1 ORDER BY timestamp DESC LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)? != 0,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Retrieves only the last message for a group (optimized for chat list preview).
    pub fn get_last_message_for_group(&self, group_id: &str) -> Result<Option<(String, String, String, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT sender_id, content, timestamp, msg_id FROM group_messages WHERE group_id = ?1 ORDER BY timestamp DESC LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![group_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Retrieves the last message for every contact in a single query (batch optimization).
    pub fn get_last_messages_all(&self) -> Result<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT peer_id, content, timestamp, is_me, msg_id FROM messages m1 WHERE timestamp = (SELECT MAX(timestamp) FROM messages m2 WHERE m2.peer_id = m1.peer_id) GROUP BY peer_id"
        )?;
        let mut map = serde_json::Map::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)? != 0,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (peer_id, content, timestamp, is_me, msg_id) = row?;
            map.insert(peer_id, serde_json::json!({
                "content": content,
                "timestamp": timestamp,
                "is_me": is_me,
                "msg_id": msg_id,
            }));
        }
        Ok(serde_json::Value::Object(map))
    }

    /// Retrieves the last message for every group in a single query (batch optimization).
    pub fn get_last_group_messages_all(&self) -> Result<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT group_id, sender_id, content, timestamp, msg_id FROM group_messages gm1 WHERE timestamp = (SELECT MAX(timestamp) FROM group_messages gm2 WHERE gm2.group_id = gm1.group_id) GROUP BY group_id"
        )?;
        let mut map = serde_json::Map::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (group_id, sender_id, content, timestamp, msg_id) = row?;
            map.insert(group_id, serde_json::json!({
                "sender_id": sender_id,
                "content": content,
                "timestamp": timestamp,
                "msg_id": msg_id,
            }));
        }
        Ok(serde_json::Value::Object(map))
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
        
        // Pre-filter: only scan groups whose members_json contains the peer_id
        // This avoids deserializing ALL groups when only a few contain this peer
        let peer_pattern = format!("%{}%", Self::escape_like(&peer_id));
        
        let mut updates = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id, members_json FROM groups WHERE members_json LIKE ?1")?;
            let rows = stmt.query_map(params![peer_pattern], |row| {
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
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT g.group_id, g.name, g.members_json, g.description, s.secret_blob 
             FROM groups g 
             LEFT JOIN group_secrets s ON g.group_id = s.group_id 
             WHERE g.group_id = ?1"
        )?;
        
        let mut rows = stmt.query_map(params![group_id], |row| {
            let gid: String = row.get(0)?;
            let name: String = row.get(1)?;
            let members_json: String = row.get(2)?;
            let description: String = row.get(3)?;
            let secret_opt: Option<Vec<u8>> = row.get(4)?;
            
            let mut secret = [0u8; 32];
            if let Some(secret_vec) = secret_opt {
                if secret_vec.len() == 32 {
                    secret.copy_from_slice(&secret_vec);
                } else {
                    crate::dispatch_debug_log(&format!("get_group: ⚠️ secret_vec length is not 32: {}", secret_vec.len()));
                }
            }
            
            Ok(GroupMeshInfo {
                group_id: gid,
                name,
                members_json,
                secret,
                description,
            })
        })?;
        
        if let Some(row) = rows.next() {
            Ok(Some(row?))
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

    pub fn store_group_message(&self, group_id: &str, sender_id: &str, msg_id: &str, content: &str, is_me: bool, reply_to: Option<&str>, timestamp: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO group_messages (group_id, sender_id, msg_id, content, is_me, status, reply_to_msg_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, CURRENT_TIMESTAMP))
             ON CONFLICT(msg_id) DO UPDATE SET content = excluded.content, reply_to_msg_id = excluded.reply_to_msg_id, timestamp = COALESCE(?8, timestamp)",
            params![group_id, sender_id, msg_id, content, is_me as i32, if is_me { 0 } else { 1 }, reply_to, timestamp],
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
        if secret.len() != 32 {
            return Err(anyhow::anyhow!("Group secret must be exactly 32 bytes"));
        }
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
        info!("[Storage] Deleted group {}: {} rows", group_id, rows);
        let msg_rows = conn.execute("DELETE FROM group_messages WHERE group_id = ?1", params![group_id])?;
        info!("[Storage] Deleted messages for group {}: {} rows", group_id, msg_rows);
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
             ON CONFLICT(group_id) DO NOTHING",
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

    /// Inserts a new handle claim. IMMMUTABLE — if the handle already exists and is verified, the insert is rejected.
    pub fn insert_handle_claim(&self, handle: &str, peer_id: &str, timestamp: i64, signatures_json: &str, verified: bool) -> Result<bool> {
        let conn = self.conn.lock();
        // Check if handle already exists and is verified (immutable)
        let existing: Option<(String, bool)> = conn.query_row(
            "SELECT peer_id, verified FROM handle_registry WHERE handle = ?1",
            params![handle],
            |row| Ok((row.get(0)?, row.get::<_, i32>(1)? != 0)),
        ).ok();

        if let Some((existing_peer, is_verified)) = existing {
            if is_verified {
                // Handle is already verified and permanently claimed — reject
                return Ok(false);
            }
            // Not yet verified — allow update (same peer re-claiming or conflict resolution)
            if existing_peer == peer_id {
                conn.execute(
                    "UPDATE handle_registry SET timestamp = ?1, signatures_json = ?2, verified = ?3 WHERE handle = ?4",
                    params![timestamp, signatures_json, verified as i32, handle],
                )?;
                return Ok(true);
            }
            // Different peer, not verified — allow overwrite (conflict resolved by RBN quorum)
        }

        conn.execute(
            "INSERT OR REPLACE INTO handle_registry (handle, peer_id, timestamp, signatures_json, verified) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![handle, peer_id, timestamp, signatures_json, verified as i32],
        )?;
        Ok(true)
    }

    /// Updates a handle claim's verification status. Only allowed if not yet verified.
    pub fn verify_handle_claim(&self, handle: &str, peer_id: &str, timestamp: i64, signatures_json: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let existing: Option<(String, bool)> = conn.query_row(
            "SELECT peer_id, verified FROM handle_registry WHERE handle = ?1",
            params![handle],
            |row| Ok((row.get(0)?, row.get::<_, i32>(1)? != 0)),
        ).ok();

        if let Some((_, is_verified)) = existing {
            if is_verified {
                return Ok(false); // Already verified — immutable
            }
        }

        conn.execute(
            "INSERT INTO handle_registry (handle, peer_id, timestamp, signatures_json, verified) VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(handle) DO UPDATE SET 
                peer_id = excluded.peer_id, 
                timestamp = excluded.timestamp, 
                signatures_json = excluded.signatures_json, 
                verified = 1",
            params![handle, peer_id, timestamp, signatures_json],
        )?;
        Ok(true)
    }

    /// Checks if a handle is already permanently claimed (verified) by ANY peer.
    pub fn is_handle_permanently_claimed(&self, handle: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let result: bool = conn.query_row(
            "SELECT COUNT(*) FROM handle_registry WHERE handle = ?1 AND verified = 1",
            params![handle],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        ).unwrap_or(false);
        Ok(result)
    }

    /// Gets the local user's verified handle (immutable once set).
    pub fn get_local_handle(&self) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let result: Option<String> = conn.query_row(
            "SELECT handle FROM profile WHERE id = 1 AND handle IS NOT NULL AND handle != ''",
            [],
            |row| row.get(0),
        ).ok();
        Ok(result)
    }

    /// Looks up a verified handle by peer_id from the local handle_registry table.
    /// Returns the handle if the peer has a verified claim, None otherwise.
    pub fn get_handle_by_peer(&self, peer_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT handle FROM handle_registry WHERE peer_id = ?1 AND verified = 1 LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![peer_id], |row| {
            row.get::<_, String>(0)
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
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
        // Check cache first
        {
            let cache = self.storage_usage_cache.lock();
            if let Some(cached) = &*cache {
                if let Some(value) = cached.get() {
                    return *value;
                }
            }
        }

        let conn = self.conn.lock();
        let drive_bytes: i64 = conn.query_row("SELECT COALESCE(SUM(total_size), 0) FROM drive_files", [], |row| row.get(0)).unwrap_or(0);
        let mesh_bytes: i64 = conn.query_row("SELECT COALESCE(SUM(LENGTH(data)), 0) FROM mesh_chunks", [], |row| row.get(0)).unwrap_or(0);
        // Use drive + mesh as approximate total (avoids unreliable fs::metadata on mobile)
        let total_disk = (drive_bytes.max(0) as u64) + (mesh_bytes.max(0) as u64);
        let result = (drive_bytes.max(0) as u64, mesh_bytes.max(0) as u64, total_disk);
        
        // Update cache
        let mut cache = self.storage_usage_cache.lock();
        *cache = Some(CachedValue::new(result, STORAGE_USAGE_CACHE_TTL));
        
        result
    }

    // --- Dead Letter Queue (Persistent) ---

    /// Store a message in the dead letter queue for crash recovery
    pub fn store_dead_letter(&self, peer_id: &str, payload: &[u8]) -> Result<()> {
        let conn = self.conn.lock();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO dead_letter_queue (peer_id, payload, queued_at) VALUES (?1, ?2, ?3)",
            params![peer_id, payload, now],
        )?;
        Ok(())
    }

    /// Fetch dead letters for a specific peer
    pub fn get_dead_letters_for_peer(&self, peer_id: &str) -> Result<Vec<(i64, Vec<u8>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, payload FROM dead_letter_queue WHERE peer_id = ?1 ORDER BY queued_at ASC LIMIT 50"
        )?;
        let rows = stmt.query_map(params![peer_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut results = Vec::new();
        for row in rows {
            if let Ok(r) = row {
                results.push(r);
            }
        }
        Ok(results)
    }

    /// Remove dead letters by ID
    pub fn remove_dead_letters(&self, ids: &[i64]) -> Result<()> {
        let conn = self.conn.lock();
        for id in ids {
            conn.execute("DELETE FROM dead_letter_queue WHERE id = ?1", params![id])?;
        }
        Ok(())
    }

    /// Clean up old dead letters (older than 24 hours)
    pub fn cleanup_old_dead_letters(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let cutoff = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64 - 86400; // 24 hours
        let deleted = conn.execute(
            "DELETE FROM dead_letter_queue WHERE queued_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    /// Get dead letter count for diagnostics
    pub fn get_dead_letter_count(&self) -> Result<usize> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dead_letter_queue",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
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
        let search_pattern = format!("%{}%", Self::escape_like(&query));
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
        let search_pattern = format!("%{}%", Self::escape_like(&query));
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
        let search_pattern = format!("%{}%", Self::escape_like(&query));
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

    pub fn search_all_messages(&self, query: &str, limit: i32) -> Result<Vec<(String, String, String, bool, i32, Option<String>, Option<String>)>> {
        let conn = self.conn.lock();
        let query_lower = query.to_lowercase();
        let is_generic = ["messages", "message", "texts", "text", "recent", "latest", "chat", "chats"].iter()
            .any(|kw| query_lower.contains(kw));

        let mut sql = String::from(
            "SELECT peer_id, content, timestamp, is_me, status, msg_id, reply_to_msg_id FROM messages"
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if !is_generic && !query.is_empty() {
            let search_pattern = format!("%{}%", Self::escape_like(&query));
            sql.push_str(" WHERE content LIKE ?");
            param_values.push(Box::new(search_pattern));
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
        param_values.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_slice.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)? != 0,
                row.get::<_, i32>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }

    pub fn search_all_group_messages(&self, query: &str, limit: i32) -> Result<Vec<(String, String, String, String, String, Option<String>)>> {
        let conn = self.conn.lock();
        let search_pattern = format!("%{}%", Self::escape_like(&query));
        let mut stmt = conn.prepare(
            "SELECT group_id, sender_id, msg_id, content, timestamp, reply_to_msg_id FROM group_messages WHERE content LIKE ?1 ORDER BY timestamp DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![search_pattern, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut messages = Vec::new();
        for row in rows { messages.push(row?); }
        Ok(messages)
    }

    pub fn search_drive_files(&self, query: &str, mime_filter: Option<&str>, days_ago: Option<i32>, limit: i32) -> Result<Vec<DriveFileMetadata>> {
        let conn = self.conn.lock();

        let type_keywords = ["photos", "images", "pictures", "videos", "clips", "files", "documents", "pdfs", "audio", "recordings"];
        let is_generic_type = type_keywords.iter().any(|kw| query.to_lowercase().contains(kw));

        let mut sql = String::from(
            "SELECT filename, file_hash, mime_type, total_size, local_path, is_backed_up, timestamp FROM drive_files WHERE 1=1"
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if is_generic_type {
            // When query is a generic type reference, just filter by mime type — no filename search
        } else if !query.is_empty() {
            sql.push_str(" AND filename LIKE ?");
            let search_pattern = format!("%{}%", Self::escape_like(&query));
            param_values.push(Box::new(search_pattern));
        }

        if let Some(mime) = mime_filter {
            sql.push_str(" AND mime_type LIKE ?");
            param_values.push(Box::new(mime.to_string()));
        }
        if let Some(days) = days_ago {
            sql.push_str(" AND timestamp >= datetime('now', ?)");
            param_values.push(Box::new(format!("-{} days", days)));
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
        param_values.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_slice.as_slice(), |row| {
            Ok(DriveFileMetadata {
                filename: row.get(0)?,
                file_hash: row.get(1)?,
                mime_type: row.get(2)?,
                total_size: row.get(3)?,
                local_path: row.get(4)?,
                is_backed_up: row.get::<_, i32>(5)? != 0,
                timestamp: row.get(6)?,
            })
        })?;
        let mut files = Vec::new();
        for row in rows { files.push(row?); }
        Ok(files)
    }

    pub fn search_contacts(&self, query: &str) -> Result<Vec<SovereignIdentity>> {
        let conn = self.conn.lock();
        let query_lower = query.to_lowercase();
        let is_generic = ["contacts", "contact", "who", "people", "person", "friends", "peers"].iter()
            .any(|kw| query_lower.contains(kw));

        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if is_generic {
            ("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, handle, prestige_tier FROM contacts ORDER BY global_name ASC".to_string(), vec![])
        } else {
            let search_pattern = format!("%{}%", Self::escape_like(&query));
            ("SELECT peer_id, p2p_pubkey, static_key, solana_address, global_name, local_alias, avatar_base64, is_anchor_capable, handle, prestige_tier FROM contacts WHERE global_name LIKE ?1 OR local_alias LIKE ?1 OR handle LIKE ?1 OR peer_id LIKE ?1".to_string(), vec![Box::new(search_pattern)])
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_slice.as_slice(), |row| {
            let pubkey: Vec<u8> = row.get(1)?;
            let static_key_blob: Vec<u8> = row.get(2)?;
            let mut sk = [0u8; 32];
            let copy_len = static_key_blob.len().min(32);
            sk[..copy_len].copy_from_slice(&static_key_blob[..copy_len]);
            Ok(SovereignIdentity {
                peer_id: row.get(0)?,
                p2p_pubkey: pubkey,
                static_key: sk,
                solana_address: row.get(3)?,
                global_name: row.get(4)?,
                local_alias: row.get(5)?,
                avatar_base64: row.get(6)?,
                is_anchor_capable: row.get::<_, i32>(7)? != 0,
                retention_seconds: 0,
                handle: row.get(8)?,
                prestige_tier: row.get::<_, Option<i32>>(9)?.map(|v| v as u8),
            })
        })?;
        let mut contacts = Vec::new();
        for row in rows { contacts.push(row?); }
        Ok(contacts)
    }

    // ── Elevated Messages ──────────────────────────────────────────────

    pub fn elevate_message(&self, chat_id: &str, msg_id: &str, content: &str, sender_id: &str, is_me: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO elevated_messages (chat_id, msg_id, content, sender_id, is_me) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![chat_id, msg_id, content, sender_id, if is_me { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn unelevate_message(&self, chat_id: &str, msg_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM elevated_messages WHERE chat_id = ?1 AND msg_id = ?2",
            params![chat_id, msg_id],
        )?;
        Ok(())
    }

    pub fn get_elevated_messages(&self, chat_id: &str) -> Result<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT msg_id, content, sender_id, is_me, timestamp, elevated_at FROM elevated_messages WHERE chat_id = ?1 ORDER BY elevated_at DESC"
        )?;
        let rows = stmt.query_map(params![chat_id], |row| {
            Ok(serde_json::json!({
                "msg_id": row.get::<_, String>(0)?,
                "content": row.get::<_, String>(1)?,
                "sender_id": row.get::<_, String>(2)?,
                "is_me": row.get::<_, i32>(3)? != 0,
                "timestamp": row.get::<_, String>(4)?,
                "elevated_at": row.get::<_, String>(5)?
            }))
        })?;
        let mut results = Vec::new();
        for row in rows { results.push(row?); }
        Ok(serde_json::Value::Array(results))
    }

    pub fn is_message_elevated(&self, chat_id: &str, msg_id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT 1 FROM elevated_messages WHERE chat_id = ?1 AND msg_id = ?2")?;
        let exists = stmt.exists(params![chat_id, msg_id])?;
        Ok(exists)
    }

    // ── Daily Rewards ──────────────────────────────────────────────────

    pub fn save_daily_cycle(&self, cycle: &crate::economy::daily_rewards::DailyCycle) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO daily_cycles (cycle_date, snapshot_balance, total_points, capped_points, intr_reward, unique_peers, is_eligible, eligibility_reason, submitted, started_at, ended_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                cycle.cycle_date,
                cycle.snapshot_balance as i64,
                cycle.total_points,
                cycle.capped_points,
                cycle.intr_reward,
                cycle.unique_peers as i64,
                if cycle.is_eligible { 1 } else { 0 },
                cycle.eligibility_reason,
                if cycle.submitted { 1 } else { 0 },
                cycle.started_at as i64,
                cycle.ended_at.map(|v| v as i64),
            ],
        )?;
        Ok(())
    }

    pub fn load_daily_cycle(&self, date: &str) -> Result<Option<crate::economy::daily_rewards::DailyCycle>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT cycle_date, snapshot_balance, total_points, capped_points, intr_reward, unique_peers, is_eligible, eligibility_reason, submitted, started_at, ended_at FROM daily_cycles WHERE cycle_date = ?1"
        )?;
        let mut rows = stmt.query_map(params![date], |row| {
            Ok(crate::economy::daily_rewards::DailyCycle {
                cycle_date: row.get(0)?,
                snapshot_balance: row.get::<_, i64>(1)? as u64,
                activities: Vec::new(),
                total_points: row.get(2)?,
                capped_points: row.get(3)?,
                intr_reward: row.get(4)?,
                unique_peers: row.get::<_, i64>(5)? as u32,
                is_eligible: row.get::<_, i64>(6)? != 0,
                eligibility_reason: row.get(7)?,
                submitted: row.get::<_, i64>(8)? != 0,
                started_at: row.get::<_, i64>(9)? as u64,
                ended_at: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn save_daily_activities(&self, date: &str, activities: &[crate::economy::daily_rewards::DailyActivityCount]) -> Result<()> {
        let conn = self.conn.lock();
        for act in activities {
            conn.execute(
                "INSERT OR REPLACE INTO daily_activity_log (cycle_date, activity_type, raw_count, capped_count, points) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![date, act.activity_type as u8, act.raw_count as i64, act.capped_count as i64, act.points],
            )?;
        }
        Ok(())
    }

    pub fn load_daily_activities(&self, date: &str) -> Result<Vec<crate::economy::daily_rewards::DailyActivityCount>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT activity_type, raw_count, capped_count, points FROM daily_activity_log WHERE cycle_date = ?1"
        )?;
        let rows = stmt.query_map(params![date], |row| {
            let at_u8: u8 = row.get(0)?;
            Ok(crate::economy::daily_rewards::DailyActivityCount {
                activity_type: crate::economy::daily_rewards::ActivityType::from_u8(at_u8)
                    .unwrap_or(crate::economy::daily_rewards::ActivityType::UptimeSeconds),
                raw_count: row.get::<_, i64>(1)? as u64,
                capped_count: row.get::<_, i64>(2)? as u64,
                points: row.get(3)?,
            })
        })?;
        let mut results = Vec::new();
        for row in rows { results.push(row?); }
        Ok(results)
    }

    pub fn save_daily_reward_config(&self, weights: &crate::economy::daily_rewards::ActivityWeights, anti_gaming: &crate::economy::daily_rewards::AntiGamingConfig) -> Result<()> {
        let conn = self.conn.lock();
        let w_json = serde_json::to_string(weights).unwrap_or_default();
        let ag_json = serde_json::to_string(anti_gaming).unwrap_or_default();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO daily_reward_config (id, weights_json, anti_gaming_json, updated_at) VALUES (1, ?1, ?2, ?3)",
            params![w_json, ag_json, now],
        )?;
        Ok(())
    }

    pub fn load_daily_reward_config(&self) -> Result<Option<(crate::economy::daily_rewards::ActivityWeights, crate::economy::daily_rewards::AntiGamingConfig)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT weights_json, anti_gaming_json FROM daily_reward_config WHERE id = 1")?;
        let mut rows = stmt.query_map([], |row| {
            let w_json: String = row.get(0)?;
            let ag_json: String = row.get(1)?;
            Ok((w_json, ag_json))
        })?;
        match rows.next() {
            Some(row) => {
                let (w_json, ag_json) = row?;
                let weights = serde_json::from_str(&w_json).unwrap_or_default();
                let anti_gaming = serde_json::from_str(&ag_json).unwrap_or_default();
                Ok(Some((weights, anti_gaming)))
            }
            None => Ok(None),
        }
    }

    pub fn get_recent_daily_cycles(&self, days: u32) -> Result<Vec<crate::economy::daily_rewards::DailyCycle>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT cycle_date, snapshot_balance, total_points, capped_points, intr_reward, unique_peers, is_eligible, eligibility_reason, submitted, started_at, ended_at FROM daily_cycles ORDER BY cycle_date DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![days as i64], |row| {
            Ok(crate::economy::daily_rewards::DailyCycle {
                cycle_date: row.get(0)?,
                snapshot_balance: row.get::<_, i64>(1)? as u64,
                activities: Vec::new(),
                total_points: row.get(2)?,
                capped_points: row.get(3)?,
                intr_reward: row.get(4)?,
                unique_peers: row.get::<_, i64>(5)? as u32,
                is_eligible: row.get::<_, i64>(6)? != 0,
                eligibility_reason: row.get(7)?,
                submitted: row.get::<_, i64>(8)? != 0,
                started_at: row.get::<_, i64>(9)? as u64,
                ended_at: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
            })
        })?;
        let mut cycles = Vec::new();
        for row in rows { cycles.push(row?); }
        Ok(cycles)
    }

    /// Persists a consolidated reward record for the given cycle date.
    ///
    /// Anti-farming guard: if `active_containers_highwater > 3`, the write is
    /// aborted and an error is returned. All point values are truncated to
    /// exactly 4 decimal places before writing to prevent cross-cycle float drift.
    pub fn save_daily_reward_record(
        &self,
        cycle_date: &str,
        total_social_points: f64,
        total_infra_points: f64,
        active_containers_highwater: u32,
        total_cycle_uptime_secs: u64,
    ) -> Result<()> {
        if active_containers_highwater > 3 {
            return Err(anyhow::anyhow!(
                "ANTI_FARMING_REJECTED: active_containers_highwater={} exceeds maximum of 3",
                active_containers_highwater
            ));
        }

        // Truncate to 4 decimal places to match the in-memory scoring engine
        let truncate = |v: f64| (v * 10_000.0).trunc() / 10_000.0;
        let social = truncate(total_social_points);
        let infra = truncate(total_infra_points);

        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO daily_reward_records \
             (cycle_date, total_social_points, total_infra_points, \
              active_containers_highwater, total_cycle_uptime_secs) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                cycle_date,
                social,
                infra,
                active_containers_highwater as i64,
                total_cycle_uptime_secs as i64,
            ],
        )?;
        Ok(())
    }

    /// Loads a consolidated reward record by cycle date.
    pub fn load_daily_reward_record(
        &self,
        cycle_date: &str,
    ) -> Result<Option<(f64, f64, u32, u64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT total_social_points, total_infra_points, \
             active_containers_highwater, total_cycle_uptime_secs \
             FROM daily_reward_records WHERE cycle_date = ?1"
        )?;
        let mut rows = stmt.query_map(params![cycle_date], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)? as u32,
                row.get::<_, i64>(3)? as u64,
            ))
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    // ── Daily Allocation Ledger ──────────────────────────────────────────

    /// Saves a batch of daily allocation results from the 10-year emission decay engine.
    pub fn save_daily_allocations(&self, cycle_date: &str, allocations: &[crate::economy::ledger_cron::DailyAllocation]) -> Result<()> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for alloc in allocations {
            conn.execute(
                "INSERT OR REPLACE INTO node_daily_allocations \
                 (cycle_date, peer_id, sol_address, raw_points, weighted_points, share_of_pool, intr_allocated, tier_multiplier, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    cycle_date,
                    alloc.peer_id,
                    alloc.sol_address,
                    alloc.raw_points,
                    alloc.weighted_points,
                    alloc.share_of_pool,
                    alloc.intr_allocated,
                    alloc.tier_multiplier as f64,
                    now
                ],
            )?;
        }
        Ok(())
    }

    /// Returns the lifetime accumulated INTR allocations for a specific peer.
    pub fn get_lifetime_allocated_intr(&self, peer_id: &str) -> Result<f64> {
        let conn = self.conn.lock();
        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(intr_allocated), 0.0) FROM node_daily_allocations WHERE peer_id = ?1",
            params![peer_id],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Returns the daily allocation for a specific peer on a specific date.
    pub fn get_daily_allocation(&self, cycle_date: &str, peer_id: &str) -> Result<Option<f64>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT intr_allocated FROM node_daily_allocations WHERE cycle_date = ?1 AND peer_id = ?2"
        )?;
        let mut rows = stmt.query_map(params![cycle_date, peer_id], |row| {
            Ok(row.get::<_, f64>(0)?)
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Returns all allocations for a given cycle date (for nightly cron aggregation).
    pub fn get_allocations_for_cycle(&self, cycle_date: &str) -> Result<Vec<(String, String, f64, f32)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT peer_id, sol_address, intr_allocated, tier_multiplier FROM node_daily_allocations WHERE cycle_date = ?1"
        )?;
        let rows = stmt.query_map(params![cycle_date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)? as f32,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}
