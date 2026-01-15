-- Add audio_mode column for audio benchmark sub-modes (STT, TTS, etc.)
ALTER TABLE benchmark_history ADD COLUMN audio_mode TEXT;

-- Index for efficient audio mode filtering
CREATE INDEX idx_audio_mode ON benchmark_history(audio_mode);
