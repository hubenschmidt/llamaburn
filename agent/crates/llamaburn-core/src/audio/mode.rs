use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioMode {
    #[default]
    Stt,
    EffectDetection,
    Tts,
    MusicSeparation,
    MusicTranscription,
    MusicGeneration,
    LlmMusicAnalysis,
}

impl AudioMode {
    pub fn label(&self) -> &'static str {
        match self {
            AudioMode::Stt => "STT",
            AudioMode::EffectDetection => "Effect Detection",
            AudioMode::Tts => "TTS",
            AudioMode::MusicSeparation => "Music Separation",
            AudioMode::MusicTranscription => "Music Transcription",
            AudioMode::MusicGeneration => "Music Generation",
            AudioMode::LlmMusicAnalysis => "LLM Music Analysis",
        }
    }

    pub fn is_implemented(&self) -> bool {
        matches!(self, AudioMode::Stt | AudioMode::EffectDetection)
    }

    pub fn all() -> &'static [AudioMode] {
        &[
            AudioMode::Stt,
            AudioMode::EffectDetection,
            AudioMode::Tts,
            AudioMode::MusicSeparation,
            AudioMode::MusicTranscription,
            AudioMode::MusicGeneration,
            AudioMode::LlmMusicAnalysis,
        ]
    }
}
