use llamaburn_benchmark::BenchmarkSummary;
use llamaburn_core::{
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkSummary, AudioMode,
    BenchmarkConfig, BenchmarkMetrics, BenchmarkType,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;

mod embedded {
    refinery::embed_migrations!("migrations");
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Migration error: {0}")]
    Migration(#[from] refinery::Error),
}

pub type Result<T> = std::result::Result<T, HistoryError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub config: BenchmarkConfig,
    pub summary: BenchmarkSummary,
    pub metrics: Vec<BenchmarkMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub audio_mode: AudioMode,
    pub model_id: String,
    pub config: AudioBenchmarkConfig,
    pub summary: AudioBenchmarkSummary,
    pub metrics: Vec<AudioBenchmarkMetrics>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub model_id: Option<String>,
    pub benchmark_type: Option<BenchmarkType>,
    pub limit: Option<u32>,
}

pub struct HistoryService {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl HistoryService {
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let path = db_path.unwrap_or_else(default_db_path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut conn = Connection::open(&path)?;
        embedded::migrations::runner().run(&mut conn)?;

        tracing::info!("History database initialized at {:?}", path);

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path,
        })
    }

    /// Get a clone of the database connection for sharing with other services
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Reset the database by dropping all tables and re-running migrations
    pub fn reset_database(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        // Drop all tables
        conn.execute("DROP TABLE IF EXISTS benchmark_history", [])?;
        conn.execute("DROP TABLE IF EXISTS settings", [])?;
        conn.execute("DROP TABLE IF EXISTS refinery_schema_history", [])?;

        // Re-run migrations
        embedded::migrations::runner().run(&mut *conn)?;

        tracing::info!("Database reset complete");
        Ok(())
    }

    pub fn insert(&self, entry: &BenchmarkHistoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let benchmark_type = serde_json::to_string(&entry.benchmark_type)?;
        let config_json = serde_json::to_string(&entry.config)?;
        let summary_json = serde_json::to_string(&entry.summary)?;
        let metrics_json = serde_json::to_string(&entry.metrics)?;

        conn.execute(
            "INSERT INTO benchmark_history (id, timestamp, benchmark_type, model_id, config_json, summary_json, metrics_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.id,
                entry.timestamp,
                benchmark_type,
                entry.model_id,
                config_json,
                summary_json,
                metrics_json,
            ],
        )?;

        tracing::debug!("Saved benchmark history entry: {}", entry.id);
        Ok(())
    }

    pub fn list(&self, filter: HistoryFilter) -> Result<Vec<BenchmarkHistoryEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT id, timestamp, benchmark_type, model_id, config_json, summary_json, metrics_json
             FROM benchmark_history WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref model_id) = filter.model_id {
            sql.push_str(" AND model_id = ?");
            params_vec.push(Box::new(model_id.clone()));
        }

        if let Some(ref benchmark_type) = filter.benchmark_type {
            let type_str = serde_json::to_string(benchmark_type).unwrap_or_default();
            sql.push_str(" AND benchmark_type = ?");
            params_vec.push(Box::new(type_str));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = conn.prepare(&sql)?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(RowData {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                benchmark_type: row.get(2)?,
                model_id: row.get(3)?,
                config_json: row.get(4)?,
                summary_json: row.get(5)?,
                metrics_json: row.get(6)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let row = row?;
            let entry = BenchmarkHistoryEntry {
                id: row.id,
                timestamp: row.timestamp,
                benchmark_type: serde_json::from_str(&row.benchmark_type).unwrap_or_default(),
                model_id: row.model_id,
                config: serde_json::from_str(&row.config_json)?,
                summary: serde_json::from_str(&row.summary_json)?,
                metrics: serde_json::from_str(&row.metrics_json)?,
            };
            entries.push(entry);
        }

        Ok(entries)
    }

    pub fn get(&self, id: &str) -> Result<Option<BenchmarkHistoryEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, timestamp, benchmark_type, model_id, config_json, summary_json, metrics_json
             FROM benchmark_history WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;

        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let entry = BenchmarkHistoryEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            benchmark_type: serde_json::from_str(&row.get::<_, String>(2)?)?,
            model_id: row.get(3)?,
            config: serde_json::from_str(&row.get::<_, String>(4)?)?,
            summary: serde_json::from_str(&row.get::<_, String>(5)?)?,
            metrics: serde_json::from_str(&row.get::<_, String>(6)?)?,
        };

        Ok(Some(entry))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM benchmark_history WHERE id = ?1", params![id])?;
        tracing::debug!("Deleted benchmark history entry: {}", id);
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM benchmark_history", [])?;
        tracing::info!("Cleared all benchmark history");
        Ok(())
    }

    /// Get the best TPS for a specific model and benchmark type
    pub fn get_best_for_model(
        &self,
        model_id: &str,
        benchmark_type: BenchmarkType,
    ) -> Result<Option<f64>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&benchmark_type)?;

        let result: std::result::Result<f64, _> = conn.query_row(
            "SELECT MAX(json_extract(summary_json, '$.avg_tps'))
             FROM benchmark_history
             WHERE model_id = ?1 AND benchmark_type = ?2",
            params![model_id, type_str],
            |row| row.get(0),
        );

        match result {
            Ok(tps) => Ok(Some(tps)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get the all-time best TPS across all models for a benchmark type
    pub fn get_all_time_best(
        &self,
        benchmark_type: BenchmarkType,
    ) -> Result<Option<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&benchmark_type)?;

        let result: std::result::Result<(String, f64), _> = conn.query_row(
            "SELECT model_id, json_extract(summary_json, '$.avg_tps') as tps
             FROM benchmark_history
             WHERE benchmark_type = ?1
             ORDER BY tps DESC
             LIMIT 1",
            params![type_str],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get leaderboard of top performers for a benchmark type
    pub fn get_leaderboard(
        &self,
        benchmark_type: BenchmarkType,
        limit: u32,
    ) -> Result<Vec<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&benchmark_type)?;

        let mut stmt = conn.prepare(
            "SELECT model_id, MAX(json_extract(summary_json, '$.avg_tps')) as best_tps
             FROM benchmark_history
             WHERE benchmark_type = ?1
             GROUP BY model_id
             ORDER BY best_tps DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![type_str, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // --- Audio History Methods ---

    /// Insert an audio benchmark result
    pub fn insert_audio(&self, entry: &AudioHistoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let benchmark_type = serde_json::to_string(&entry.benchmark_type)?;
        let audio_mode = serde_json::to_string(&entry.audio_mode)?;
        let config_json = serde_json::to_string(&entry.config)?;
        let summary_json = serde_json::to_string(&entry.summary)?;
        let metrics_json = serde_json::to_string(&entry.metrics)?;

        conn.execute(
            "INSERT INTO benchmark_history (id, timestamp, benchmark_type, audio_mode, model_id, config_json, summary_json, metrics_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.id,
                entry.timestamp,
                benchmark_type,
                audio_mode,
                entry.model_id,
                config_json,
                summary_json,
                metrics_json,
            ],
        )?;

        tracing::debug!("Saved audio benchmark history entry: {}", entry.id);
        Ok(())
    }

    /// Get the best RTF for a specific model and audio mode (lower is better)
    pub fn get_best_audio_for_model(
        &self,
        model_id: &str,
        audio_mode: AudioMode,
    ) -> Result<Option<f64>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&BenchmarkType::Audio)?;
        let mode_str = serde_json::to_string(&audio_mode)?;

        let result: std::result::Result<f64, _> = conn.query_row(
            "SELECT MIN(json_extract(summary_json, '$.avg_rtf'))
             FROM benchmark_history
             WHERE model_id = ?1 AND benchmark_type = ?2 AND audio_mode = ?3",
            params![model_id, type_str, mode_str],
            |row| row.get(0),
        );

        match result {
            Ok(rtf) => Ok(Some(rtf)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get the all-time best RTF for an audio mode (lower is better)
    pub fn get_all_time_best_audio(
        &self,
        audio_mode: AudioMode,
    ) -> Result<Option<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&BenchmarkType::Audio)?;
        let mode_str = serde_json::to_string(&audio_mode)?;

        let result: std::result::Result<(String, f64), _> = conn.query_row(
            "SELECT model_id, json_extract(summary_json, '$.avg_rtf') as rtf
             FROM benchmark_history
             WHERE benchmark_type = ?1 AND audio_mode = ?2
             ORDER BY rtf ASC
             LIMIT 1",
            params![type_str, mode_str],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get audio leaderboard sorted by RTF ascending (lower is better)
    pub fn get_audio_leaderboard(
        &self,
        audio_mode: AudioMode,
        limit: u32,
    ) -> Result<Vec<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let type_str = serde_json::to_string(&BenchmarkType::Audio)?;
        let mode_str = serde_json::to_string(&audio_mode)?;

        let mut stmt = conn.prepare(
            "SELECT model_id, MIN(json_extract(summary_json, '$.avg_rtf')) as best_rtf
             FROM benchmark_history
             WHERE benchmark_type = ?1 AND audio_mode = ?2
             GROUP BY model_id
             ORDER BY best_rtf ASC
             LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![type_str, mode_str, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get the database path
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }
}

struct RowData {
    id: String,
    timestamp: i64,
    benchmark_type: String,
    model_id: String,
    config_json: String,
    summary_json: String,
    metrics_json: String,
}

fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("llamaburn")
        .join("history.db")
}
