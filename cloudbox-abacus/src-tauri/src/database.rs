//! SQLite storage: persistent operation queue and remote file index.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Operation {
    pub id: i64,
    pub op_type: String,
    pub local_path: String,
    pub remote_path: String,
    pub status: String,
    pub retry_count: i64,
    pub error_msg: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileIndexEntry {
    pub remote_path: String,
    pub local_path: String,
    pub size: i64,
    pub mtime: String,
    pub checksum: Option<String>,
    pub sync_status: String,
    pub updated_at: String,
}

pub struct Database {
    conn: Connection,
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

impl Database {
    pub fn open_default() -> Result<Self> {
        Self::open(&crate::config::app_dir()?.join("cloudbox.db"))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS operations (
                id INTEGER PRIMARY KEY,
                op_type TEXT, local_path TEXT, remote_path TEXT,
                status TEXT, retry_count INTEGER DEFAULT 0,
                error_msg TEXT, created_at TEXT, updated_at TEXT
             );
             CREATE TABLE IF NOT EXISTS file_index (
                id INTEGER PRIMARY KEY,
                remote_path TEXT UNIQUE,
                local_path TEXT,
                size INTEGER, mtime TEXT, checksum TEXT,
                sync_status TEXT, updated_at TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_ops_status ON operations(status);",
        )?;
        Ok(Self { conn })
    }

    // ---------- operations ----------

    pub fn add_operation(&self, op_type: &str, local: &str, remote: &str, status: &str) -> Result<i64> {
        let ts = now();
        self.conn.execute(
            "INSERT INTO operations (op_type, local_path, remote_path, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![op_type, local, remote, status, ts],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Adds a pending operation unless an identical one is already pending.
    pub fn enqueue_unique(&self, op_type: &str, local: &str, remote: &str) -> Result<()> {
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM operations WHERE op_type=?1 AND local_path=?2 AND status='pending' LIMIT 1",
                params![op_type, local],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            self.add_operation(op_type, local, remote, "pending")?;
        }
        Ok(())
    }

    pub fn update_operation(&self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        let retry_inc = if status == "error" { 1 } else { 0 };
        self.conn.execute(
            "UPDATE operations SET status=?1, error_msg=?2, retry_count=retry_count+?3, updated_at=?4 WHERE id=?5",
            params![status, error, retry_inc, now(), id],
        )?;
        Ok(())
    }

    /// Marks all pending operations for a local path prefix as done.
    pub fn complete_pending_under(&self, local_prefix: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE operations SET status='done', updated_at=?1 WHERE status='pending' AND local_path LIKE ?2",
            params![now(), format!("{local_prefix}%")],
        )?)
    }

    fn map_op(r: &rusqlite::Row) -> rusqlite::Result<Operation> {
        Ok(Operation {
            id: r.get(0)?,
            op_type: r.get(1)?,
            local_path: r.get(2)?,
            remote_path: r.get(3)?,
            status: r.get(4)?,
            retry_count: r.get(5)?,
            error_msg: r.get(6)?,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    }

    pub fn list_operations(&self, limit: u32) -> Result<Vec<Operation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, op_type, local_path, remote_path, status, retry_count, error_msg, created_at, updated_at
             FROM operations ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], Self::map_op)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pending_operations(&self) -> Result<Vec<Operation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, op_type, local_path, remote_path, status, retry_count, error_msg, created_at, updated_at
             FROM operations WHERE status='pending' ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], Self::map_op)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete_operation(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM operations WHERE id=?1", params![id])?;
        Ok(())
    }

    /// Keeps the operation log bounded.
    pub fn prune_operations(&self, keep: u32) -> Result<()> {
        self.conn.execute(
            "DELETE FROM operations WHERE status != 'pending' AND id NOT IN
             (SELECT id FROM operations ORDER BY id DESC LIMIT ?1)",
            params![keep],
        )?;
        Ok(())
    }

    // ---------- file index ----------

    pub fn upsert_file(&self, remote: &str, local: &str, size: i64, mtime: &str, checksum: Option<&str>, status: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO file_index (remote_path, local_path, size, mtime, checksum, sync_status, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(remote_path) DO UPDATE SET local_path=excluded.local_path, size=excluded.size,
               mtime=excluded.mtime, checksum=excluded.checksum, sync_status=excluded.sync_status,
               updated_at=excluded.updated_at",
            params![remote, local, size, mtime, checksum, status, now()],
        )?;
        Ok(())
    }

    pub fn get_file(&self, remote: &str) -> Result<Option<FileIndexEntry>> {
        Ok(self
            .conn
            .query_row(
                "SELECT remote_path, local_path, size, mtime, checksum, sync_status, updated_at
                 FROM file_index WHERE remote_path=?1",
                params![remote],
                |r| {
                    Ok(FileIndexEntry {
                        remote_path: r.get(0)?,
                        local_path: r.get(1)?,
                        size: r.get(2)?,
                        mtime: r.get(3)?,
                        checksum: r.get(4)?,
                        sync_status: r.get(5)?,
                        updated_at: r.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn delete_file(&self, remote: &str) -> Result<()> {
        self.conn.execute("DELETE FROM file_index WHERE remote_path=?1", params![remote])?;
        Ok(())
    }

    pub fn count_files(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM file_index", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_crud() {
        let db = Database::open_in_memory().unwrap();
        let id = db.add_operation("upload", "/a", "/r/a", "pending").unwrap();
        db.enqueue_unique("upload", "/a", "/r/a").unwrap();
        assert_eq!(db.pending_operations().unwrap().len(), 1);
        db.update_operation(id, "error", Some("boom")).unwrap();
        let ops = db.list_operations(10).unwrap();
        assert_eq!(ops[0].retry_count, 1);
        assert_eq!(ops[0].error_msg.as_deref(), Some("boom"));
        db.delete_operation(id).unwrap();
        assert!(db.list_operations(10).unwrap().is_empty());
    }

    #[test]
    fn file_index_upsert() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_file("/r/a", "/a", 10, "1", None, "synced").unwrap();
        db.upsert_file("/r/a", "/a", 20, "2", None, "synced").unwrap();
        assert_eq!(db.count_files().unwrap(), 1);
        assert_eq!(db.get_file("/r/a").unwrap().unwrap().size, 20);
    }
}
