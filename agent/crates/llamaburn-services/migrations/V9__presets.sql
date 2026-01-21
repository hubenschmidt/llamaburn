-- Presets table for saved benchmark configurations
CREATE TABLE benchmark_presets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    model_id TEXT NOT NULL,
    language TEXT NOT NULL,
    temperature REAL NOT NULL,
    max_tokens INTEGER,
    problem_ids TEXT NOT NULL
);

-- Link history entries to presets for comparison
ALTER TABLE benchmark_history ADD COLUMN preset_id TEXT;
