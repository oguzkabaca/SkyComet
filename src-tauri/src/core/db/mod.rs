pub mod migrations;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("app data directory is unavailable")]
    AppDataDirUnavailable,
}

pub type DbResult<T> = Result<T, DbError>;

#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> DbResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        migrations::run_migrations(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;
        migrations::run_migrations(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    fn apply_pragmas(conn: &Connection) -> DbResult<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn with_conn<F, R>(&self, f: F) -> DbResult<R>
    where
        F: FnOnce(&Connection) -> DbResult<R>,
    {
        let guard = self
            .inner
            .lock()
            .map_err(|e| DbError::Migration(format!("db mutex poisoned: {e}")))?;
        f(&guard)
    }
}

pub fn resolve_db_path(app_data_dir: Option<PathBuf>) -> DbResult<PathBuf> {
    if cfg!(debug_assertions) {
        // Anchor dev DB on the workspace root (one level above the src-tauri
        // crate) so it is independent of the current working directory —
        // `cargo tauri dev` (CWD=src-tauri) and root-level tooling point at
        // the same file.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or(manifest_dir);
        Ok(workspace_root.join("dev-data").join("skycomet.db"))
    } else {
        app_data_dir
            .map(|dir| dir.join("skycomet.db"))
            .ok_or(DbError::AppDataDirUnavailable)
    }
}
