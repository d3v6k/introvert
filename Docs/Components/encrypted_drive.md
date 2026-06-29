# Sovereign Encrypted Drive

## 1. Overview
The Sovereign Drive is Introvert's encrypted file storage system. Files are stored locally with metadata in the SQLCipher database and organized into context-aware subfolders.

## 2. Architecture

### Storage Layers
1. **Physical Storage:** Files stored in `${appDocumentsDir}/drive/`
2. **Metadata:** SQLCipher `drive_files` table (encrypted at rest)
3. **DHT Chunking:** Optional mesh-wide storage via `mesh_chunks` table

### File Organization
```
${appDocumentsDir}/drive/
├── General_Media/           # Unclassified files
├── {ContactAlias}_Media/    # Files from specific contacts
├── {GroupName}_Media/       # Files from group chats
└── ...
```

## 3. Database Schema

### `drive_files` Table
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

### `mesh_chunks` Table
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

## 4. Operations

### Upload
1. User selects file via `file_picker`
2. File copied to persistent drive directory
3. SHA-256 hash computed
4. Metadata registered with `driveAddFile`
5. File available for sharing

### Download/Share
1. User selects file from drive
2. Local path retrieved from metadata
3. File shared via chat without re-upload
4. Recipient receives via file transfer protocol

### Delete
1. User selects "Remove from Drive"
2. Physical file deleted via `File(localPath).deleteSync()`
3. Metadata removed from `drive_files` table

## 5. Integration

### Chat Integration
- Files from chat auto-registered in drive
- Forward from drive to chat without re-upload
- Thumbnails cached for instant display

### Group Integration
- Group files organized in `{GroupName}_Media/`
- Shared group secret encrypts file metadata
- Members can access via group permissions

### Cross-Device Sync
- Drive metadata synced via mesh
- Physical files transfer on-demand
- DHT chunking for mesh-wide availability

## 6. Security

### Encryption
- All metadata encrypted in SQLCipher
- File content encrypted during transfer (AES-GCM)
- Local files stored in app sandbox (OS-level protection)

### Access Control
- Only owner can modify drive contents
- Shared files require explicit sharing action
- Group files accessible to group members only

## 7. Performance

### Storage Limits
- Limited only by device storage
- DHT chunks limited to 1GB per file
- Automatic cleanup of expired chunks

### Transfer Speeds
- Direct P2P: 14+ Mbps
- Relayed: 0.3-1 Mbps
- WebRTC: 1-5 Mbps

## 8. iOS/macOS Sandbox Handling

### Dynamic Paths
- App container UUID changes on every launch
- Absolute paths in database become invalid
- `resolveSandboxPath` utility rewrites paths dynamically

### Resolution Logic
```dart
String resolveSandboxPath(String storedPath) {
  // Replace Library/Application Support/ with current session path
  // Replace Documents/ with current session path
  // Strip bundle ID prefix for debug builds
}
```

## 9. UI Integration

### DriveTab Features
- File listing with search
- Upload/download with progress
- Swarm capacity display
- Active transfer monitoring
- Auto-refresh every 5 seconds

### File Bubble UX
- Images: Thumbnail only (filename hidden)
- Videos: Thumbnail with play overlay
- Documents: Icon + filename + extension
- All: Hash verification status
