use llamaburn_benchmark::BenchmarkSummary;
use llamaburn_core::{
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkSummary, AudioMode,
    BenchmarkConfig, BenchmarkMetrics, BenchmarkType, CodeBenchmarkConfig, CodeBenchmarkMetrics,
    CodeBenchmarkSummary, EffectDetectionResult, EffectDetectionTool, Language,
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
    #[error("Lock poisoned")]
    LockPoisoned,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub language: Language,
    pub config: CodeBenchmarkConfig,
    pub summary: CodeBenchmarkSummary,
    pub metrics: Vec<CodeBenchmarkMetrics>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionHistoryEntry {
    pub id: i64,
    pub tool: EffectDetectionTool,
    pub audio_path: String,
    pub result: EffectDetectionResult,
    pub created_at: i64,
}

/// Status of a batch benchmark run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchStatus {
    Running,
    Paused,
    Completed,
}

impl BatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchStatus::Running => "running",
            BatchStatus::Paused => "paused",
            BatchStatus::Completed => "completed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => BatchStatus::Running,
            "paused" => BatchStatus::Paused,
            "completed" => BatchStatus::Completed,
            _ => BatchStatus::Paused,
        }
    }
}

/// A single benchmark combination in a matrix run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCombo {
    pub model: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// Persisted state for a resumable batch benchmark session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchState {
    pub session_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: BatchStatus,
    pub selected_models: Vec<String>,
    pub selected_languages: Vec<Language>,
    pub selected_temperatures: Vec<f32>,
    pub selected_max_tokens: Vec<u32>,
    pub selected_problem_ids: Vec<String>,
    pub auto_run_tests: bool,
    pub skip_on_error: bool,
    pub pending_combos: Vec<BatchCombo>,
    pub queue_total: usize,
    pub queue_completed: usize,
    pub failed_combo: Option<BatchCombo>,
    pub error_message: Option<String>,
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
        let mut conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

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
            // Skip rows that fail deserialization (e.g., Audio/Code entries with different schemas)
            let Ok(config) = serde_json::from_str(&row.config_json) else {
                continue;
            };
            let Ok(summary) = serde_json::from_str(&row.summary_json) else {
                continue;
            };
            let Ok(metrics) = serde_json::from_str(&row.metrics_json) else {
                continue;
            };
            let entry = BenchmarkHistoryEntry {
                id: row.id,
                timestamp: row.timestamp,
                benchmark_type: serde_json::from_str(&row.benchmark_type).unwrap_or_default(),
                model_id: row.model_id,
                config,
                summary,
                metrics,
            };
            entries.push(entry);
        }

        Ok(entries)
    }

    pub fn get(&self, id: &str) -> Result<Option<BenchmarkHistoryEntry>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        conn.execute("DELETE FROM benchmark_history WHERE id = ?1", params![id])?;
        tracing::debug!("Deleted benchmark history entry: {}", id);
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
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

    // --- Code History Methods ---

    /// Insert a code benchmark result
    pub fn insert_code(&self, entry: &CodeHistoryEntry) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

        let benchmark_type = serde_json::to_string(&entry.benchmark_type)?;
        let language = serde_json::to_string(&entry.language)?;
        let config_json = serde_json::to_string(&entry.config)?;
        let summary_json = serde_json::to_string(&entry.summary)?;
        let metrics_json = serde_json::to_string(&entry.metrics)?;

        conn.execute(
            "INSERT INTO benchmark_history (id, timestamp, benchmark_type, language, model_id, config_json, summary_json, metrics_json, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.timestamp,
                benchmark_type,
                language,
                entry.model_id,
                config_json,
                summary_json,
                metrics_json,
                entry.session_id,
            ],
        )?;

        tracing::debug!("Saved code benchmark history entry: {}", entry.id);
        Ok(())
    }

    /// Get the best pass_rate for a specific model and language (higher is better)
    pub fn get_best_code_for_model(
        &self,
        model_id: &str,
        language: Language,
    ) -> Result<Option<f64>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let type_str = serde_json::to_string(&BenchmarkType::Code)?;
        let lang_str = serde_json::to_string(&language)?;

        let result: std::result::Result<f64, _> = conn.query_row(
            "SELECT MAX(json_extract(summary_json, '$.pass_rate'))
             FROM benchmark_history
             WHERE model_id = ?1 AND benchmark_type = ?2 AND language = ?3",
            params![model_id, type_str, lang_str],
            |row| row.get(0),
        );

        match result {
            Ok(pass_rate) => Ok(Some(pass_rate)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get code leaderboard sorted by pass_rate descending (higher is better)
    pub fn get_code_leaderboard(
        &self,
        language: Language,
        limit: u32,
    ) -> Result<Vec<(String, f64)>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let type_str = serde_json::to_string(&BenchmarkType::Code)?;
        let lang_str = serde_json::to_string(&language)?;

        let mut stmt = conn.prepare(
            "SELECT model_id, MAX(json_extract(summary_json, '$.pass_rate')) as best_pass_rate
             FROM benchmark_history
             WHERE benchmark_type = ?1 AND language = ?2
             GROUP BY model_id
             ORDER BY best_pass_rate DESC
             LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![type_str, lang_str, limit], |row| {
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

    /// List audio benchmark history entries
    pub fn list_audio(&self, limit: Option<u32>) -> Result<Vec<AudioHistoryEntry>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let type_str = serde_json::to_string(&BenchmarkType::Audio)?;

        let mut sql = String::from(
            "SELECT id, timestamp, benchmark_type, audio_mode, model_id, config_json, summary_json, metrics_json
             FROM benchmark_history WHERE benchmark_type = ?",
        );

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![type_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, timestamp, benchmark_type, audio_mode, model_id, config_json, summary_json, metrics_json) = row?;
            let Ok(audio_mode) = audio_mode.map(|s| serde_json::from_str(&s)).transpose() else {
                continue;
            };
            let Ok(config) = serde_json::from_str(&config_json) else {
                continue;
            };
            let Ok(summary) = serde_json::from_str(&summary_json) else {
                continue;
            };
            let Ok(metrics) = serde_json::from_str(&metrics_json) else {
                continue;
            };
            entries.push(AudioHistoryEntry {
                id,
                timestamp,
                benchmark_type: serde_json::from_str(&benchmark_type).unwrap_or_default(),
                audio_mode: audio_mode.unwrap_or(AudioMode::Stt),
                model_id,
                config,
                summary,
                metrics,
            });
        }

        Ok(entries)
    }

    /// List code benchmark history entries
    pub fn list_code(&self, limit: Option<u32>) -> Result<Vec<CodeHistoryEntry>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let type_str = serde_json::to_string(&BenchmarkType::Code)?;

        let mut sql = String::from(
            "SELECT id, timestamp, benchmark_type, language, model_id, config_json, summary_json, metrics_json, session_id
             FROM benchmark_history WHERE benchmark_type = ?",
        );

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![type_str], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, timestamp, benchmark_type, language, model_id, config_json, summary_json, metrics_json, session_id) = row?;
            let Ok(language) = language.map(|s| serde_json::from_str(&s)).transpose() else {
                continue;
            };
            let Ok(config) = serde_json::from_str(&config_json) else {
                continue;
            };
            let Ok(summary) = serde_json::from_str(&summary_json) else {
                continue;
            };
            let Ok(metrics) = serde_json::from_str(&metrics_json) else {
                continue;
            };
            entries.push(CodeHistoryEntry {
                id,
                timestamp,
                benchmark_type: serde_json::from_str(&benchmark_type).unwrap_or_default(),
                model_id,
                language: language.unwrap_or(Language::Python),
                config,
                summary,
                metrics,
                session_id,
            });
        }

        Ok(entries)
    }

    // --- Effect Detection History Methods ---

    /// Save an effect detection result
    pub fn save_effect_detection(
        &self,
        tool: EffectDetectionTool,
        audio_path: &str,
        result: &EffectDetectionResult,
    ) -> Result<i64> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

        let tool_str = serde_json::to_string(&tool)?;
        let effects_json = serde_json::to_string(&result.effects)?;

        conn.execute(
            "INSERT INTO effect_detection_history (tool, audio_path, effects_json, processing_time_ms, audio_duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                tool_str,
                audio_path,
                effects_json,
                result.processing_time_ms,
                result.audio_duration_ms,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Get recent effect detection history
    pub fn get_effect_detection_history(&self, limit: u32) -> Result<Vec<EffectDetectionHistoryEntry>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;

        let mut stmt = conn.prepare(
            "SELECT id, tool, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at
             FROM effect_detection_history
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            let id: i64 = row.get(0)?;
            let tool_str: String = row.get(1)?;
            let audio_path: String = row.get(2)?;
            let effects_json: String = row.get(3)?;
            let processing_time_ms: f64 = row.get(4)?;
            let audio_duration_ms: f64 = row.get(5)?;
            let created_at: i64 = row.get(6)?;

            Ok((id, tool_str, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (id, tool_str, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at) = row?;
            let tool: EffectDetectionTool = serde_json::from_str(&tool_str).unwrap_or_default();
            let effects = serde_json::from_str(&effects_json).unwrap_or_default();

            results.push(EffectDetectionHistoryEntry {
                id,
                tool,
                audio_path,
                result: EffectDetectionResult {
                    tool,
                    effects,
                    processing_time_ms,
                    audio_duration_ms,
                    embeddings: None,
                    applied_effects: None,
                    signal_analysis: None,
                    llm_description: None,
                    llm_model_used: None,
                    embedding_distance: None,
                    cosine_similarity: None,
                },
                created_at,
            });
        }

        Ok(results)
    }

    /// Get effect detection history for a specific tool
    pub fn get_effect_detection_by_tool(
        &self,
        tool: EffectDetectionTool,
        limit: u32,
    ) -> Result<Vec<EffectDetectionHistoryEntry>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let tool_str = serde_json::to_string(&tool)?;

        let mut stmt = conn.prepare(
            "SELECT id, tool, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at
             FROM effect_detection_history
             WHERE tool = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![tool_str, limit], |row| {
            let id: i64 = row.get(0)?;
            let tool_str: String = row.get(1)?;
            let audio_path: String = row.get(2)?;
            let effects_json: String = row.get(3)?;
            let processing_time_ms: f64 = row.get(4)?;
            let audio_duration_ms: f64 = row.get(5)?;
            let created_at: i64 = row.get(6)?;

            Ok((id, tool_str, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (id, tool_str, audio_path, effects_json, processing_time_ms, audio_duration_ms, created_at) = row?;
            let tool: EffectDetectionTool = serde_json::from_str(&tool_str).unwrap_or_default();
            let effects = serde_json::from_str(&effects_json).unwrap_or_default();

            results.push(EffectDetectionHistoryEntry {
                id,
                tool,
                audio_path,
                result: EffectDetectionResult {
                    tool,
                    effects,
                    processing_time_ms,
                    audio_duration_ms,
                    embeddings: None,
                    applied_effects: None,
                    signal_analysis: None,
                    llm_description: None,
                    llm_model_used: None,
                    embedding_distance: None,
                    cosine_similarity: None,
                },
                created_at,
            });
        }

        Ok(results)
    }

    // ========== Batch State Methods ==========

    /// Insert a new batch state
    pub fn insert_batch(&self, batch: &BatchState) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        conn.execute(
            "INSERT INTO batch_state (
                session_id, created_at, updated_at, status,
                selected_models, selected_languages, selected_temperatures,
                selected_max_tokens, selected_problem_ids,
                auto_run_tests, skip_on_error,
                pending_combos, queue_total, queue_completed,
                failed_combo, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                batch.session_id,
                batch.created_at,
                batch.updated_at,
                batch.status.as_str(),
                serde_json::to_string(&batch.selected_models)?,
                serde_json::to_string(&batch.selected_languages)?,
                serde_json::to_string(&batch.selected_temperatures)?,
                serde_json::to_string(&batch.selected_max_tokens)?,
                serde_json::to_string(&batch.selected_problem_ids)?,
                batch.auto_run_tests as i32,
                batch.skip_on_error as i32,
                serde_json::to_string(&batch.pending_combos)?,
                batch.queue_total as i64,
                batch.queue_completed as i64,
                batch.failed_combo.as_ref().map(|c| serde_json::to_string(c)).transpose()?,
                batch.error_message.as_ref(),
            ],
        )?;
        Ok(())
    }

    /// Update an existing batch state
    pub fn update_batch(&self, batch: &BatchState) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        conn.execute(
            "UPDATE batch_state SET
                updated_at = ?2, status = ?3,
                pending_combos = ?4, queue_completed = ?5,
                failed_combo = ?6, error_message = ?7
            WHERE session_id = ?1",
            params![
                batch.session_id,
                batch.updated_at,
                batch.status.as_str(),
                serde_json::to_string(&batch.pending_combos)?,
                batch.queue_completed as i64,
                batch.failed_combo.as_ref().map(|c| serde_json::to_string(c)).transpose()?,
                batch.error_message.as_ref(),
            ],
        )?;
        Ok(())
    }

    /// Get all incomplete batches (running or paused)
    pub fn get_incomplete_batches(&self) -> Result<Vec<BatchState>> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT session_id, created_at, updated_at, status,
                    selected_models, selected_languages, selected_temperatures,
                    selected_max_tokens, selected_problem_ids,
                    auto_run_tests, skip_on_error,
                    pending_combos, queue_total, queue_completed,
                    failed_combo, error_message
             FROM batch_state
             WHERE status IN ('running', 'paused')
             ORDER BY updated_at DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(BatchStateRow {
                session_id: row.get(0)?,
                created_at: row.get(1)?,
                updated_at: row.get(2)?,
                status: row.get(3)?,
                selected_models: row.get(4)?,
                selected_languages: row.get(5)?,
                selected_temperatures: row.get(6)?,
                selected_max_tokens: row.get(7)?,
                selected_problem_ids: row.get(8)?,
                auto_run_tests: row.get(9)?,
                skip_on_error: row.get(10)?,
                pending_combos: row.get(11)?,
                queue_total: row.get(12)?,
                queue_completed: row.get(13)?,
                failed_combo: row.get(14)?,
                error_message: row.get(15)?,
            })
        })?;

        let mut batches = Vec::new();
        for row in rows {
            let row = row?;
            batches.push(BatchState {
                session_id: row.session_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                status: BatchStatus::from_str(&row.status),
                selected_models: serde_json::from_str(&row.selected_models)?,
                selected_languages: serde_json::from_str(&row.selected_languages)?,
                selected_temperatures: serde_json::from_str(&row.selected_temperatures)?,
                selected_max_tokens: serde_json::from_str(&row.selected_max_tokens)?,
                selected_problem_ids: serde_json::from_str(&row.selected_problem_ids)?,
                auto_run_tests: row.auto_run_tests != 0,
                skip_on_error: row.skip_on_error != 0,
                pending_combos: serde_json::from_str(&row.pending_combos)?,
                queue_total: row.queue_total as usize,
                queue_completed: row.queue_completed as usize,
                failed_combo: row.failed_combo.map(|s| serde_json::from_str(&s)).transpose()?,
                error_message: row.error_message,
            });
        }
        Ok(batches)
    }

    /// Delete a batch state
    pub fn delete_batch(&self, session_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| HistoryError::LockPoisoned)?;
        conn.execute("DELETE FROM batch_state WHERE session_id = ?1", params![session_id])?;
        Ok(())
    }
}

struct BatchStateRow {
    session_id: String,
    created_at: i64,
    updated_at: i64,
    status: String,
    selected_models: String,
    selected_languages: String,
    selected_temperatures: String,
    selected_max_tokens: String,
    selected_problem_ids: String,
    auto_run_tests: i32,
    skip_on_error: i32,
    pending_combos: String,
    queue_total: i64,
    queue_completed: i64,
    failed_combo: Option<String>,
    error_message: Option<String>,
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
