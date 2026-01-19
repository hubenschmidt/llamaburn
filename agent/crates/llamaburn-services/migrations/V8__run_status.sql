-- Add status column to benchmark_history
ALTER TABLE benchmark_history ADD COLUMN status TEXT NOT NULL DEFAULT 'success';
