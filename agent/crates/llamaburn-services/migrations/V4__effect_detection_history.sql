-- Effect detection history table
CREATE TABLE effect_detection_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tool TEXT NOT NULL,
    audio_path TEXT NOT NULL,
    effects_json TEXT NOT NULL,
    processing_time_ms REAL NOT NULL,
    audio_duration_ms REAL NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Index for efficient tool filtering
CREATE INDEX idx_effect_tool ON effect_detection_history(tool);
CREATE INDEX idx_effect_created_at ON effect_detection_history(created_at DESC);
