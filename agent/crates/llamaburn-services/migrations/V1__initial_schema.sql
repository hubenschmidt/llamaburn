-- Initial schema for benchmark history
CREATE TABLE benchmark_history (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    benchmark_type TEXT NOT NULL,
    model_id TEXT NOT NULL,
    config_json TEXT NOT NULL,
    summary_json TEXT NOT NULL,
    metrics_json TEXT NOT NULL
);

CREATE INDEX idx_model_id ON benchmark_history(model_id);
CREATE INDEX idx_benchmark_type ON benchmark_history(benchmark_type);
CREATE INDEX idx_timestamp ON benchmark_history(timestamp DESC);
