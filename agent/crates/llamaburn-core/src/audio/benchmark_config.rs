use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{AudioMode, AudioSource, WhisperModel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkConfig {
    pub audio_mode: AudioMode,
    pub audio_source: AudioSource,
    pub model_size: Option<WhisperModel>,
    pub audio_path: PathBuf,
    pub language: Option<String>,
    pub iterations: u32,
    pub warmup_runs: u32,
}

impl Default for AudioBenchmarkConfig {
    fn default() -> Self {
        Self {
            audio_mode: AudioMode::default(),
            audio_source: AudioSource::default(),
            model_size: None,
            audio_path: PathBuf::new(),
            language: None,
            iterations: 3,
            warmup_runs: 1,
        }
    }
}
