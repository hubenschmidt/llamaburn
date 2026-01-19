-- Add session_id to group multi-model benchmark runs
ALTER TABLE benchmark_history ADD COLUMN session_id TEXT;
