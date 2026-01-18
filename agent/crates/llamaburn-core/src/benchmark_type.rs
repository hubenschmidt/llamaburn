use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkType {
    #[default]
    Text,
    Image,
    Audio,
    Video,
    Graphics3D,
    Code,
}

impl BenchmarkType {
    pub fn label(&self) -> &'static str {
        match self {
            BenchmarkType::Text => "Text",
            BenchmarkType::Image => "Image",
            BenchmarkType::Audio => "Audio",
            BenchmarkType::Video => "Video",
            BenchmarkType::Graphics3D => "3D",
            BenchmarkType::Code => "Code",
        }
    }

    pub fn is_implemented(&self) -> bool {
        matches!(self, BenchmarkType::Text | BenchmarkType::Audio | BenchmarkType::Code)
    }

    pub fn all() -> &'static [BenchmarkType] {
        &[
            BenchmarkType::Text,
            BenchmarkType::Image,
            BenchmarkType::Audio,
            BenchmarkType::Video,
            BenchmarkType::Graphics3D,
            BenchmarkType::Code,
        ]
    }
}
