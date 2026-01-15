use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, SettingsError>;

/// Key-value settings store backed by SQLite
pub struct SettingsService {
    conn: Arc<Mutex<Connection>>,
}

impl SettingsService {
    /// Create a new SettingsService sharing a connection with HistoryService
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Get a setting value by key
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();

        let result: std::result::Result<String, _> = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a setting value (insert or update)
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;

        tracing::debug!("Setting saved: {} = {}", key, value);
        Ok(())
    }

    /// Delete a setting by key
    pub fn delete(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        tracing::debug!("Setting deleted: {}", key);
        Ok(())
    }

    /// List all settings
    pub fn list(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

// Well-known setting keys
pub mod keys {
    pub const HF_API_KEY: &str = "hf_api_key";
    pub const OLLAMA_HOST: &str = "ollama_host";
}
