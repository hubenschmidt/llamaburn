use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EffectDetectionTool {
    FxEncoderPlusPlus,
    OpenAmp,
    #[default]
    Llm2FxTools,
}

impl EffectDetectionTool {
    pub fn label(&self) -> &'static str {
        match self {
            EffectDetectionTool::FxEncoderPlusPlus => "Fx-Encoder++ (Sony)",
            EffectDetectionTool::OpenAmp => "OpenAmp",
            EffectDetectionTool::Llm2FxTools => "LLM2Fx-Tools",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            EffectDetectionTool::FxEncoderPlusPlus => {
                "Contrastive learning for effect representation"
            }
            EffectDetectionTool::OpenAmp => "Framework for effect detection models",
            EffectDetectionTool::Llm2FxTools => "Dry/Wet comparison (detects processing)",
        }
    }

    pub fn all() -> &'static [EffectDetectionTool] {
        &[
            EffectDetectionTool::FxEncoderPlusPlus,
            EffectDetectionTool::OpenAmp,
            EffectDetectionTool::Llm2FxTools,
        ]
    }
}
