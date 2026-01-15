use std::sync::mpsc::{channel, Receiver};
use std::thread;

use serde::Deserialize;
use tracing::debug;

use llamaburn_core::WhisperModel;

use crate::{OllamaClient, OllamaShowResponse};

#[derive(Debug, Clone, Default)]
pub struct ModelInfo {
    pub model_id: String,
    // Ollama metadata
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub family: Option<String>,
    pub format: Option<String>,
    // HuggingFace metadata
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

#[derive(Debug, Deserialize)]
struct HfSearchResult {
    id: String,
    downloads: Option<u64>,
}

impl ModelInfo {
    pub fn from_ollama(model_id: &str, resp: &OllamaShowResponse) -> Self {
        Self {
            model_id: model_id.to_string(),
            parameter_size: Some(resp.details.parameter_size.clone()),
            quantization: Some(resp.details.quantization_level.clone()),
            family: Some(resp.details.family.clone()),
            format: Some(resp.details.format.clone()),
            ..Default::default()
        }
    }

    pub fn hf_url(&self) -> Option<String> {
        self.hf_repo
            .as_ref()
            .map(|repo| format!("https://huggingface.co/{}", repo))
    }
}

pub struct ModelInfoService {
    ollama: OllamaClient,
}

impl ModelInfoService {
    pub fn new(ollama: OllamaClient) -> Self {
        Self { ollama }
    }

    pub fn fetch_info(&self, model_id: &str) -> Option<ModelInfo> {
        let mut info = self
            .ollama
            .show_model(model_id)
            .ok()
            .map(|resp| ModelInfo::from_ollama(model_id, &resp))?;

        // Try to fetch HuggingFace metadata
        if let Some(hf_info) = self.fetch_huggingface(model_id, &info) {
            info.hf_repo = hf_info.hf_repo;
            info.hf_downloads = hf_info.hf_downloads;
            info.hf_likes = hf_info.hf_likes;
            info.hf_license = hf_info.hf_license;
            info.hf_author = hf_info.hf_author;
            info.hf_pipeline = hf_info.hf_pipeline;
            info.hf_gated = hf_info.hf_gated;
            info.hf_last_modified = hf_info.hf_last_modified;
        }

        Some(info)
    }

    pub fn fetch_info_async(&self, model_id: &str) -> Receiver<Option<ModelInfo>> {
        let (tx, rx) = channel();
        let model = model_id.to_string();
        let host = self.ollama.host().to_string();

        thread::spawn(move || {
            let client = OllamaClient::new(host);
            let service = ModelInfoService::new(client);
            let result = service.fetch_info(&model);
            let _ = tx.send(result);
        });

        rx
    }

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

    fn fetch_huggingface(&self, model_id: &str, info: &ModelInfo) -> Option<ModelInfo> {
        // Strategy 1: Parse hf.co/ prefix from model name
        if let Some(repo) = Self::parse_hf_model_name(model_id) {
            debug!("Found HF repo from model name: {}", repo);
            return Self::fetch_hf_by_repo(&repo);
        }

        // Strategy 2: Search HuggingFace API
        if let Some(repo) = Self::search_huggingface(info) {
            debug!("Found HF repo from search: {}", repo);
            return Self::fetch_hf_by_repo(&repo);
        }

        // Strategy 3: Heuristic mapping
        if let Some(repo) = Self::guess_hf_repo(info) {
            debug!("Using heuristic HF repo: {}", repo);
            return Self::fetch_hf_by_repo(&repo);
        }

        None
    }

    /// Parse model names like "hf.co/unsloth/Nemotron-3-Nano-30B-A3B-GGUF:Q4_1"
    fn parse_hf_model_name(model_id: &str) -> Option<String> {
        let stripped = model_id.strip_prefix("hf.co/")?;
        // Remove tag after colon: "unsloth/Nemotron-3-Nano-30B-A3B-GGUF:Q4_1" -> "unsloth/Nemotron-3-Nano-30B-A3B-GGUF"
        let repo = stripped.split(':').next()?;
        // Remove -GGUF suffix for cleaner HF lookup
        let clean_repo = repo.trim_end_matches("-GGUF").trim_end_matches("-gguf");
        Some(clean_repo.to_string())
    }

    /// Search HuggingFace API for matching model
    fn search_huggingface(info: &ModelInfo) -> Option<String> {
        let family = info.family.as_deref()?;
        let size = info.parameter_size.as_deref().unwrap_or("");

        // Build search query
        let query = format!("{} {}", family, size);
        let url = format!(
            "https://huggingface.co/api/models?search={}&sort=downloads&direction=-1&limit=5",
            urlencoding::encode(&query)
        );

        debug!("Searching HuggingFace: {}", url);

        let response = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .ok()?;

        let results: Vec<HfSearchResult> = response.into_json().ok()?;

        // Pick the most downloaded result that looks like a base model
        results
            .into_iter()
            .filter(|r| !r.id.to_lowercase().contains("gguf"))
            .filter(|r| !r.id.to_lowercase().contains("gptq"))
            .filter(|r| !r.id.to_lowercase().contains("awq"))
            .max_by_key(|r| r.downloads.unwrap_or(0))
            .map(|r| r.id)
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

        let last_modified = hf.last_modified.map(|s| {
            s.split('T').next().unwrap_or(&s).to_string()
        });

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

    fn guess_hf_repo(info: &ModelInfo) -> Option<String> {
        let family = info.family.as_deref()?;
        let size = info.parameter_size.as_deref().unwrap_or("");

        // Common mappings based on Ollama model family
        let repo = match family.to_lowercase().as_str() {
            "llama" => {
                let size_num = size.trim_end_matches('B');
                format!("meta-llama/Llama-3.1-{size_num}B")
            }
            "gemma" | "gemma2" => {
                let size_num = size.trim_end_matches('B');
                format!("google/gemma-2-{size_num}b")
            }
            "qwen2" | "qwen" => {
                let size_num = size.trim_end_matches('B');
                format!("Qwen/Qwen2.5-{size_num}B")
            }
            "mistral" => "mistralai/Mistral-7B-v0.1".to_string(),
            "mixtral" => "mistralai/Mixtral-8x7B-v0.1".to_string(),
            "phi3" | "phi" => "microsoft/Phi-3-mini-4k-instruct".to_string(),
            "codellama" => {
                let size_num = size.trim_end_matches('B');
                format!("codellama/CodeLlama-{size_num}b-hf")
            }
            "deepseek" | "deepseek2" => "deepseek-ai/DeepSeek-V2-Lite".to_string(),
            "starcoder" | "starcoder2" => "bigcode/starcoder2-15b".to_string(),
            "falcon" => "tiiuae/falcon-7b".to_string(),
            "vicuna" => "lmsys/vicuna-7b-v1.5".to_string(),
            _ => return None,
        };

        Some(repo)
    }
}

impl Default for ModelInfoService {
    fn default() -> Self {
        Self::new(OllamaClient::default())
    }
}
