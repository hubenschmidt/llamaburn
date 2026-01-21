//! Model info service for fetching HuggingFace metadata for Whisper models

use std::sync::mpsc::{channel, Receiver};
use std::thread;

use serde::Deserialize;

use llamaburn_core::WhisperModel;

#[derive(Debug, Clone, Default)]
pub struct ModelInfo {
    pub model_id: String,
    pub hf_repo: Option<String>,
    pub hf_downloads: Option<u64>,
    pub hf_likes: Option<u64>,
    pub hf_license: Option<String>,
    pub hf_author: Option<String>,
    pub hf_pipeline: Option<String>,
    pub hf_gated: Option<String>,
    pub hf_last_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HfModelResponse {
    id: Option<String>,
    downloads: Option<u64>,
    likes: Option<u64>,
    author: Option<String>,
    pipeline_tag: Option<String>,
    gated: Option<serde_json::Value>,
    #[serde(rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(rename = "cardData")]
    card_data: Option<HfCardData>,
}

#[derive(Debug, Deserialize)]
struct HfCardData {
    license: Option<String>,
}

impl ModelInfo {
    pub fn hf_url(&self) -> Option<String> {
        self.hf_repo
            .as_ref()
            .map(|repo| format!("https://huggingface.co/{}", repo))
    }
}

pub struct ModelInfoService;

impl ModelInfoService {
    /// Fetch model info for a Whisper model from HuggingFace
    pub fn fetch_whisper_info(model: WhisperModel) -> Option<ModelInfo> {
        let repo = match model {
            WhisperModel::Tiny => "openai/whisper-tiny",
            WhisperModel::Base => "openai/whisper-base",
            WhisperModel::Small => "openai/whisper-small",
            WhisperModel::Medium => "openai/whisper-medium",
            WhisperModel::Large => "openai/whisper-large",
            WhisperModel::LargeV3 => "openai/whisper-large-v3",
            WhisperModel::LargeV3Turbo => "openai/whisper-large-v3-turbo",
        };

        let mut info = Self::fetch_hf_by_repo(repo)?;
        info.model_id = model.label().to_string();
        Some(info)
    }

    /// Async version of fetch_whisper_info
    pub fn fetch_whisper_info_async(model: WhisperModel) -> Receiver<Option<ModelInfo>> {
        let (tx, rx) = channel();

        thread::spawn(move || {
            let result = Self::fetch_whisper_info(model);
            let _ = tx.send(result);
        });

        rx
    }

    fn fetch_hf_by_repo(repo: &str) -> Option<ModelInfo> {
        let url = format!("https://huggingface.co/api/models/{}", repo);

        let response = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .ok()?;

        let hf: HfModelResponse = response.into_json().ok()?;

        let gated = hf.gated.and_then(|v| match v {
            serde_json::Value::Bool(b) => b.then_some("Yes".to_string()),
            serde_json::Value::String(s) => Some(s),
            _ => None,
        });

        let last_modified = hf
            .last_modified
            .map(|s| s.split('T').next().unwrap_or(&s).to_string());

        Some(ModelInfo {
            hf_repo: hf.id,
            hf_downloads: hf.downloads,
            hf_likes: hf.likes,
            hf_license: hf.card_data.and_then(|c| c.license),
            hf_author: hf.author,
            hf_pipeline: hf.pipeline_tag,
            hf_gated: gated,
            hf_last_modified: last_modified,
            ..Default::default()
        })
    }
}
