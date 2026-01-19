-- Batch state for resumable benchmark sessions
CREATE TABLE batch_state (
    session_id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    status TEXT NOT NULL,  -- 'running', 'paused', 'completed'
    -- Selections
    selected_models TEXT NOT NULL,       -- JSON array
    selected_languages TEXT NOT NULL,    -- JSON array
    selected_temperatures TEXT NOT NULL, -- JSON array
    selected_max_tokens TEXT NOT NULL,   -- JSON array
    selected_problem_ids TEXT NOT NULL,  -- JSON array
    auto_run_tests INTEGER NOT NULL,
    skip_on_error INTEGER NOT NULL,      -- 0=pause on error, 1=skip and continue
    -- Queue
    pending_combos TEXT NOT NULL,        -- JSON array of BenchmarkCombo
    queue_total INTEGER NOT NULL,
    queue_completed INTEGER NOT NULL,
    -- Failure info
    failed_combo TEXT,                   -- JSON (nullable)
    error_message TEXT                   -- Last error (nullable)
);

CREATE INDEX idx_batch_status ON batch_state(status);
