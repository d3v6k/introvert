# Database Schema Reference

## Overview

Introvert uses SQLCipher (encrypted SQLite) for local storage. The database is encrypted with AES-256-CBC using a key derived from the master seed via HKDF-SHA256.

## Schema Version

Current version: **1.0.0**

Migrations are handled in `src/storage.rs` during engine startup.

## Tables

### messages
Stores 1-on-1 chat history.

```sql
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    peer_id TEXT NOT NULL,
    msg_id TEXT UNIQUE,
    content TEXT NOT NULL,
    reply_to_msg_id TEXT,
    is_me INTEGER DEFAULT 0,
    status INTEGER DEFAULT 0,  -- 0=Sent, 1=Delivered, 2=Read
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_messages_peer_time ON messages (peer_id, timestamp DESC);
```

### group_messages
Gossipsub group chat history.

```sql
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
```

### contacts
Sovereign contacts registry.

```sql
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
    retention_hours INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### profile
Local user identity details.

```sql
CREATE TABLE IF NOT EXISTS profile (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    name TEXT,
    handle TEXT UNIQUE,
    avatar_base64 TEXT,
    privacy_mode INTEGER DEFAULT 1
);
```

### groups
Gossipsub mesh group room registry.

```sql
CREATE TABLE IF NOT EXISTS groups (
    group_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    members_json TEXT NOT NULL,
    retention_hours INTEGER DEFAULT 0,
    muted_members_json TEXT DEFAULT '[]',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### group_secrets
Cryptographic keys for mesh groups.

```sql
CREATE TABLE IF NOT EXISTS group_secrets (
    group_id TEXT PRIMARY KEY,
    secret_blob BLOB NOT NULL
);
```

### deleted_groups
Deletion log preventing re-association.

```sql
CREATE TABLE IF NOT EXISTS deleted_groups (
    group_id TEXT PRIMARY KEY,
    deleted_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### drive_files
Encrypted drive files metadata.

```sql
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
CREATE INDEX IF NOT EXISTS idx_drive_hash ON drive_files (file_hash);
```

### mesh_chunks
Communal 1GB DHT chunk storage registry.

```sql
CREATE TABLE IF NOT EXISTS mesh_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_hash TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    data BLOB NOT NULL,
    timestamp INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mesh_chunks_file ON mesh_chunks (file_hash);
CREATE INDEX IF NOT EXISTS idx_mesh_chunks_timestamp ON mesh_chunks (timestamp);
```

### mailbox_messages
Encrypted store-and-forward mailbox cache on RBN/Anchor nodes.

```sql
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
```

### mailbox_stats
Anchor metrics tracking.

```sql
CREATE TABLE IF NOT EXISTS mailbox_stats (
    date TEXT PRIMARY KEY,
    storage_bytes_seconds INTEGER DEFAULT 0
);
```

### crypto_sessions
Asymmetric Noise session states cache.

```sql
CREATE TABLE IF NOT EXISTS crypto_sessions (
    session_id TEXT PRIMARY KEY,
    data BLOB NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### session_cache
Ephemeral handshakes cache.

```sql
CREATE TABLE IF NOT EXISTS session_cache (
    peer_id TEXT PRIMARY KEY,
    session_blob BLOB NOT NULL,
    last_active DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### reward_log
Record of bytes relayed for rewards computations.

```sql
CREATE TABLE IF NOT EXISTS reward_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    relayed_bytes INTEGER NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### economy_meta
Symmetric registry for economy keys and Intro-Claw config.

```sql
CREATE TABLE IF NOT EXISTS economy_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

#### Intro-Claw Config Keys

| Key | Type | Description |
|-----|------|-------------|
| `intro_claw_active` | `true`/`false` | Master toggle for Intro-Claw engine |
| `intro_claw_ai_mode` | `true`/`false` | Hybrid AI mode enabled state |
| `intro_claw_api_key` | string (encrypted) | OpenAI-compatible API key (SQLCipher-encrypted) |
| `intro_claw_endpoint` | string | OpenAI-compatible API endpoint URL |

### handle_registry
PoW INR handle witness consensus database.

```sql
CREATE TABLE IF NOT EXISTS handle_registry (
    handle TEXT PRIMARY KEY,
    peer_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    signatures_json TEXT NOT NULL,
    verified INTEGER DEFAULT 0
);
```

### push_tokens
Push tokens registered with RBNs.

```sql
CREATE TABLE IF NOT EXISTS push_tokens (
    peer_id TEXT PRIMARY KEY,
    device_type TEXT NOT NULL,
    push_token TEXT NOT NULL,
    last_seen DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### message_reactions
Emoji reaction feedback database.

```sql
CREATE TABLE IF NOT EXISTS message_reactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    emoji TEXT NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(msg_id, sender_id)
);
CREATE INDEX IF NOT EXISTS idx_reactions_msg ON message_reactions (msg_id);
```

## Relationships

```
messages.peer_id ──► contacts.peer_id
group_messages.group_id ──► groups.group_id
group_messages.sender_id ──► contacts.peer_id
group_secrets.group_id ──► groups.group_id
deleted_groups.group_id ──► groups.group_id
drive_files.file_hash ──► mesh_chunks.file_hash
handle_registry.peer_id ──► contacts.peer_id
push_tokens.peer_id ──► contacts.peer_id
message_reactions.msg_id ──► messages.msg_id / group_messages.msg_id
```

## Indexes

| Table | Index | Columns | Purpose |
|-------|-------|---------|---------|
| messages | idx_messages_peer_time | peer_id, timestamp DESC | Fast message lookup |
| group_messages | idx_group_messages_id | group_id, timestamp DESC | Fast group message lookup |
| drive_files | idx_drive_hash | file_hash | Fast file lookup |
| mesh_chunks | idx_mesh_chunks_file | file_hash | Fast chunk lookup |
| mesh_chunks | idx_mesh_chunks_timestamp | timestamp | TTL cleanup |
| mailbox_messages | idx_mailbox_recipient | recipient_hash | Fast mailbox lookup |
| mailbox_messages | idx_mailbox_ttl | ttl_expiry | TTL cleanup |
| message_reactions | idx_reactions_msg | msg_id | Fast reaction lookup |

## Encryption

### Database Encryption
- **Algorithm:** AES-256-CBC
- **Key Derivation:** HKDF-SHA256 from master seed
- **Key Domain:** `introvert_storage_key`
- **PRAGMA:** `key = x'hex_key'`

### Column Encryption
Some columns contain additional application-level encryption:
- `messages.content` — Encrypted with Noise session key
- `group_messages.content` — Encrypted with group secret
- `drive_files.local_path` — Plain text (file system encryption)
- `mesh_chunks.data` — Encrypted with file key

## Migrations

### Adding New Tables
1. Add `CREATE TABLE IF NOT EXISTS` in `bootstrap()`
2. Add indexes as needed
3. Test with existing database
4. Document in CHANGELOG.md

### Modifying Tables
1. Add `ALTER TABLE` statements
2. Handle backward compatibility
3. Test migration path
4. Version the schema if needed

## Backup

### Export
```bash
# Via SQLCipher
sqlcipher introvert.db ".dump" > backup.sql
```

### Import
```bash
# Via SQLCipher
sqlcipher introvert.db ".read backup.sql"
```

### Automated Backup
```bash
# Cron job for daily backup
0 2 * * * sqlite3 /opt/introvert/data/introvert.db ".backup /backup/introvert-$(date +\%Y\%m\%d).db"
```
