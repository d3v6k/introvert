// ============================================================
// EXTRACTED FROM: src/storage.rs (3324 lines total)
// Date: 2026-07-18
// Purpose: Expert consultation for same-network file transfer delay
// ============================================================

// ------------------------------------------------------------
// SECTION 1: pending_file_chunks Table Schema (lines 331-355)
// ------------------------------------------------------------
/*
CREATE TABLE IF NOT EXISTS pending_file_chunks (
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
CREATE INDEX IF NOT EXISTS idx_pending_chunks_transfer ON pending_file_chunks(transfer_id, chunk_index);

-- Added later (idempotent migration):
ALTER TABLE pending_file_chunks ADD COLUMN in_flight_since INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pending_file_chunks ADD COLUMN connection_id TEXT DEFAULT NULL;
*/


// ------------------------------------------------------------
// SECTION 2: enqueue_pending_chunk (lines 1402-1409)
// Adds a chunk to the persistent queue
// ------------------------------------------------------------
/*
pub fn enqueue_pending_chunk(&self, transfer_id: &str, peer_id: &str, chunk_index: u32, chunk_data: &[u8]) -> Result<()> {
    let conn = self.conn.lock();
    conn.execute(
        "INSERT OR REPLACE INTO pending_file_chunks (transfer_id, peer_id, chunk_index, chunk_data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![transfer_id, peer_id, chunk_index as i32, chunk_data, chrono::Utc::now().timestamp()],
    )?;
    Ok(())
}
*/

// NOTE: Uses INSERT OR REPLACE, so re-enqueuing the same (transfer_id, chunk_index)
// overwrites the existing row. This means the UNIQUE constraint is used for upsert,
// not for dedup prevention. The same chunk can be enqueued multiple times.


// ------------------------------------------------------------
// SECTION 3: dequeue_pending_chunks (lines 1411-1457)
// Selects chunks to send, marks them as in-flight
// ------------------------------------------------------------
/*
pub fn dequeue_pending_chunks(&self, peer_id: &str, limit: usize) -> Result<Vec<(String, u32, Vec<u8>)>> {
    let mut conn = self.conn.lock();
    let now = chrono::Utc::now().timestamp();
    let stale_cutoff = now - 30; // in-flight claims older than 30s are considered dead

    let tx = conn.transaction()?;
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM pending_file_chunks
             WHERE peer_id = ?1 AND (in_flight_since = 0 OR in_flight_since < ?2)
             ORDER BY transfer_id ASC, chunk_index ASC LIMIT ?3"
        )?;
        // ... collect IDs
    };
    
    // Mark selected chunks as in-flight
    tx.execute(
        &format!("UPDATE pending_file_chunks SET in_flight_since = ?1 WHERE id IN ({})", placeholders),
        // ... params
    )?;
    
    // Fetch chunk data
    let mut stmt = tx.prepare(&format!(
        "SELECT transfer_id, chunk_index, chunk_data FROM pending_file_chunks WHERE id IN ({})", placeholders
    ))?;
    // ... collect results
    
    tx.commit()?;
    Ok(result)
}
*/

// KEY OBSERVATION: dequeue_pending_chunks marks chunks as in_flight but does NOT
// delete them. Deletion only happens in remove_pending_chunk() which is called
// from forward_to_mesh() on gossipsub publish success. If the circuit drops
// before publish succeeds, the in_flight_since resets after 30s and the chunk
// is re-selected on the next dequeue call.


// ------------------------------------------------------------
// SECTION 4: Chunk Lifecycle Functions (lines 1459-1526)
// ------------------------------------------------------------
/*
pub fn release_in_flight_chunk(&self, transfer_id: &str, chunk_index: u32) -> Result<()> {
    // Resets in_flight_since to 0, making the chunk eligible for re-send
    conn.execute("UPDATE pending_file_chunks SET in_flight_since = 0 WHERE transfer_id = ?1 AND chunk_index = ?2", ...);
}

pub fn remove_pending_chunk(&self, transfer_id: &str, chunk_index: u32) -> Result<()> {
    // Permanently deletes a chunk from the queue
    conn.execute("DELETE FROM pending_file_chunks WHERE transfer_id = ?1 AND chunk_index = ?2", ...);
}

pub fn increment_chunk_retry(&self, transfer_id: &str, chunk_index: u32, max_retries: i32) -> Result<i32> {
    // Increments retry count, deletes if max_retries exceeded
}

pub fn remove_pending_chunks_for_transfer(&self, transfer_id: &str) -> Result<()> {
    // Deletes ALL chunks for a transfer (called on FileTransferComplete)
}

pub fn cleanup_stale_pending_chunks(&self, max_age_secs: i64) -> Result<usize> {
    // Deletes chunks older than max_age_secs
}

pub fn has_pending_chunks_for_peer(&self, peer_id: &str) -> Result<bool> {
    // Checks if any chunks are pending for a peer
}
*/


// ------------------------------------------------------------
// SECTION 5: The Dedup Gap
// ------------------------------------------------------------
// The flow for a successful chunk delivery:
// 1. enqueue_pending_chunk() — adds to DB
// 2. dequeue_pending_chunks() — selects, marks in_flight
// 3. forward_to_mesh() → gossipsub.publish()
// 4. On success: remove_pending_chunk() — deletes from DB
//
// The flow for a FAILED chunk delivery (circuit drop):
// 1. enqueue_pending_chunk() — adds to DB
// 2. dequeue_pending_chunks() — selects, marks in_flight
// 3. forward_to_mesh() → gossipsub.publish() — FAILS (circuit dropped)
// 4. release_in_flight_chunk() — resets in_flight to 0
// 5. Circuit reconnects → InboundCircuitEstablished
// 6. dequeue_pending_chunks() — selects SAME chunks again (in_flight=0)
// 7. Steps 3-6 repeat → same chunks sent 3-9 times
//
// The 30-second stale_cutoff in dequeue_pending_chunks means that even
// if release_in_flight_chunk() is not called, the chunks become eligible
// again after 30 seconds.
