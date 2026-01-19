-- Add language column for code benchmark language tracking
ALTER TABLE benchmark_history ADD COLUMN language TEXT;

-- Index for efficient language filtering
CREATE INDEX idx_language ON benchmark_history(language);
