// SQLite database module — stores settings, history, shortcuts, and model config

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Database handle wrapping SQLite connection
pub struct Database {
    conn: Mutex<Connection>,
}

/// M6 fix: Helper to lock mutex, recovering from poison
fn lock_conn(conn: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
    conn.lock().unwrap_or_else(|e| e.into_inner())
}

impl Database {
    /// Initialize database at the given path, creating tables if needed
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    /// Create required tables
    fn init_tables(&self) -> anyhow::Result<()> {
        let conn = lock_conn(&self.conn);
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                language TEXT,
                engine TEXT,
                confidence REAL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS shortcuts (
                id TEXT PRIMARY KEY,
                accelerator TEXT NOT NULL,
                description TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS model_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                encrypted INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            )",
            [],
        )?;
        Ok(())
    }

    /// Get a setting value by key
    pub fn get_setting(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = lock_conn(&self.conn);
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let result = stmt
            .query_row([key], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(result)
    }

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = lock_conn(&self.conn);
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, strftime('%s','now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            [key, value],
        )?;
        Ok(())
    }

    /// Get all settings as a JSON object
    pub fn get_all_settings(&self) -> anyhow::Result<serde_json::Value> {
        let conn = lock_conn(&self.conn);
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut map = serde_json::Map::new();
        for row in rows {
            let (key, value) = row?;
            map.insert(key, serde_json::Value::String(value));
        }
        Ok(serde_json::Value::Object(map))
    }

    /// Insert a recognition result into history
    pub fn insert_history(
        &self,
        text: &str,
        language: Option<&str>,
        engine: Option<&str>,
        confidence: Option<f64>,
    ) -> anyhow::Result<()> {
        let conn = lock_conn(&self.conn);
        conn.execute(
            "INSERT INTO history (text, language, engine, confidence) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![text, language, engine, confidence],
        )?;
        Ok(())
    }

    /// Get recognition history
    /// M1 fix: Accept i64 instead of usize for rusqlite compatibility
    pub fn get_history(&self, limit: i64) -> anyhow::Result<Vec<serde_json::Value>> {
        let conn = lock_conn(&self.conn);
        let mut stmt = conn.prepare(
            "SELECT id, text, language, engine, confidence, created_at
             FROM history ORDER BY created_at DESC LIMIT ?1"
        )?;
        // m12 fix: Use Value::Null for NaN confidence instead of 0
        let rows = stmt.query_map([limit], |row| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".to_string(), serde_json::Value::Number(row.get::<_, i64>(0)?.into()));
            obj.insert("text".to_string(), serde_json::Value::String(row.get(1)?));
            obj.insert("language".to_string(), row.get::<_, Option<String>>(2)?
                .map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
            obj.insert("engine".to_string(), row.get::<_, Option<String>>(3)?
                .map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
            obj.insert("confidence".to_string(), row.get::<_, Option<f64>>(4)?
                .and_then(|v| serde_json::Number::from_f64(v))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null));
            obj.insert("created_at".to_string(), serde_json::Value::Number(row.get::<_, i64>(5)?.into()));
            Ok(serde_json::Value::Object(obj))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Clear all recognition history
    pub fn clear_history(&self) -> anyhow::Result<()> {
        let conn = lock_conn(&self.conn);
        conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Save model configuration (API key, endpoint, etc.)
    /// C7 fix: API keys should be stored with encrypted=1 flag
    pub fn save_model_config(&self, key: &str, value: &str, encrypted: bool) -> anyhow::Result<()> {
        let conn = lock_conn(&self.conn);
        conn.execute(
            "INSERT INTO model_config (key, value, encrypted, updated_at)
             VALUES (?1, ?2, ?3, strftime('%s','now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, encrypted = excluded.encrypted, updated_at = excluded.updated_at",
            rusqlite::params![key, value, encrypted as i32],
        )?;
        Ok(())
    }

    /// Get model configuration
    pub fn get_model_config(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = lock_conn(&self.conn);
        let mut stmt = conn.prepare("SELECT value FROM model_config WHERE key = ?1")?;
        stmt.query_row([key], |row| row.get::<_, String>(0))
            .optional()
            .map_err(|e| e.into())
    }
}
