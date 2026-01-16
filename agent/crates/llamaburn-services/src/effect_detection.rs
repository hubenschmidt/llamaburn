//! Audio effect detection service using Python ML tools
//!
//! Supports three detection tools:
//! - Fx-Encoder++ (Sony Research) - Contrastive learning for effect embeddings
//! - OpenAmp - Framework for effect detection models
//! - LLM2Fx-Tools - LLM-based effect chain prediction

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use thiserror::Error;
use tracing::{debug, error, info};

use llamaburn_core::{DetectedEffect, EffectDetectionResult, EffectDetectionTool};

/// Get the path to the LlamaBurn venv Python, or fall back to system python3
fn get_python_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let venv_python = PathBuf::from(format!("{}/.llamaburn/venv/bin/python", home));

    if venv_python.exists() {
        return venv_python;
    }

    PathBuf::from("python3")
}

#[derive(Error, Debug)]
pub enum EffectDetectionError {
    #[error("Python not found - is Python 3 installed?")]
    PythonNotFound,
    #[error("Tool not available: {0}. Install with: {1}")]
    ToolNotAvailable(String, String),
    #[error("Failed to execute detection: {0}")]
    ExecutionFailed(String),
    #[error("Failed to parse output: {0}")]
    ParseError(String),
    #[error("Audio file not found: {0}")]
    AudioNotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct EffectDetectionService {
    tool: EffectDetectionTool,
}

impl EffectDetectionService {
    pub fn new(tool: EffectDetectionTool) -> Self {
        Self { tool }
    }

    pub fn set_tool(&mut self, tool: EffectDetectionTool) {
        self.tool = tool;
    }

    pub fn tool(&self) -> EffectDetectionTool {
        self.tool
    }

    /// Check if a specific tool is available on the system
    pub fn is_tool_available(tool: EffectDetectionTool) -> bool {
        let check_script = match tool {
            EffectDetectionTool::FxEncoderPlusPlus => {
                "import fxencoder_plusplus; print('ok')"
            }
            EffectDetectionTool::OpenAmp => {
                "import openamp; print('ok')"
            }
            EffectDetectionTool::Llm2FxTools => {
                "import llm2fx; print('ok')"
            }
        };

        let python = get_python_path();
        let result = Command::new(&python)
            .args(["-c", check_script])
            .output();

        match result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Get installation instructions for a tool (using venv for modern Linux)
    pub fn install_instructions(tool: EffectDetectionTool) -> &'static str {
        match tool {
            EffectDetectionTool::FxEncoderPlusPlus => {
                "# Create venv and install Fx-Encoder++\n\
                 python3 -m venv ~/.llamaburn/venv\n\
                 source ~/.llamaburn/venv/bin/activate\n\
                 pip install fxencoder_plusplus"
            }
            EffectDetectionTool::OpenAmp => {
                "# Create venv and install OpenAmp\n\
                 python3 -m venv ~/.llamaburn/venv\n\
                 source ~/.llamaburn/venv/bin/activate\n\
                 pip install openamp librosa"
            }
            EffectDetectionTool::Llm2FxTools => {
                "# Create venv and install LLM2Fx-Tools\n\
                 python3 -m venv ~/.llamaburn/venv\n\
                 source ~/.llamaburn/venv/bin/activate\n\
                 pip install llm2fx-tools\n\
                 # See: https://arxiv.org/abs/2512.01559"
            }
        }
    }

    /// Detect audio effects in the given audio file
    pub fn detect(&self, audio_path: &Path) -> Result<EffectDetectionResult, EffectDetectionError> {
        if !audio_path.exists() {
            return Err(EffectDetectionError::AudioNotFound(
                audio_path.display().to_string(),
            ));
        }

        info!(tool = ?self.tool, path = %audio_path.display(), "Starting effect detection");
        let start = Instant::now();

        let result = match self.tool {
            EffectDetectionTool::FxEncoderPlusPlus => self.detect_fx_encoder(audio_path),
            EffectDetectionTool::OpenAmp => self.detect_openamp(audio_path),
            EffectDetectionTool::Llm2FxTools => self.detect_llm2fx(audio_path),
        };

        let elapsed = start.elapsed();
        info!(elapsed_ms = elapsed.as_millis(), "Effect detection completed");

        result
    }

    fn detect_fx_encoder(&self, audio_path: &Path) -> Result<EffectDetectionResult, EffectDetectionError> {
        let script = format!(
            r#"
import json
import sys
import os
import io
import warnings

# Suppress all warnings
warnings.filterwarnings('ignore')
os.environ['TRANSFORMERS_VERBOSITY'] = 'error'

def main():
    import torch
    import librosa

    # Try to import the library
    try:
        from fxencoder_plusplus import load_model
    except ImportError as e:
        return {{"error": f"fxencoder_plusplus not installed: {{e}}"}}

    # Load audio (must be stereo for Fx-Encoder++)
    try:
        wav, sr = librosa.load("{}", sr=44100, mono=False)
    except Exception as e:
        return {{"error": f"Failed to load audio: {{e}}"}}

    if wav.ndim == 1:
        wav = torch.from_numpy(wav).unsqueeze(0).repeat(2, 1)  # mono to stereo
    else:
        wav = torch.from_numpy(wav)
    wav = wav.unsqueeze(0)  # [1, 2, seq_len]

    device = 'cuda' if torch.cuda.is_available() else 'cpu'
    wav = wav.to(device)

    # Load model (suppress stdout spam from library)
    try:
        old_stdout = sys.stdout
        sys.stdout = io.StringIO()
        model = load_model('default', device=device)
        sys.stdout = old_stdout
    except Exception as e:
        sys.stdout = old_stdout
        return {{"error": f"Failed to load model: {{e}}"}}

    # Get embeddings
    try:
        with torch.no_grad():
            fx_emb = model.get_fx_embedding(wav)
        embeddings = fx_emb.cpu().detach().numpy().flatten().tolist()[:32]
    except Exception as e:
        return {{"error": f"Failed to get embeddings: {{e}}"}}

    return {{
        "effects": [{{"name": "fx_embedding", "confidence": 1.0}}],
        "embeddings": embeddings
    }}

try:
    result = main()
    print(json.dumps(result))
    sys.stdout.flush()
except Exception as e:
    print(json.dumps({{"error": f"Unexpected error: {{e}}"}}))
    sys.stdout.flush()
"#,
            audio_path.display()
        );

        self.run_python_script(&script, audio_path)
    }

    fn detect_openamp(&self, audio_path: &Path) -> Result<EffectDetectionResult, EffectDetectionError> {
        let script = format!(
            r#"
import json
import sys
try:
    from openamp import FxEncoder
    import librosa

    audio, sr = librosa.load("{}", sr=None)
    encoder = FxEncoder.from_pretrained()
    result = encoder.extract(audio, sr)

    effects = []
    for name, conf in result.get('detected_effects', {{}}).items():
        effects.append({{"name": name, "confidence": float(conf)}})

    output = {{
        "effects": effects,
        "embeddings": result.get('encodings', [])
    }}
    print(json.dumps(output))
except ImportError as e:
    print(json.dumps({{"error": f"Import failed: {{e}}"}}))
    sys.exit(1)
except Exception as e:
    print(json.dumps({{"error": str(e)}}))
    sys.exit(1)
"#,
            audio_path.display()
        );

        self.run_python_script(&script, audio_path)
    }

    fn detect_llm2fx(&self, audio_path: &Path) -> Result<EffectDetectionResult, EffectDetectionError> {
        let script = format!(
            r#"
import json
import sys
try:
    from llm2fx import EffectPredictor

    predictor = EffectPredictor.from_pretrained()
    result = predictor.analyze_audio("{}")

    effects = []
    for effect in result.get('effect_chain', []):
        effects.append({{
            "name": effect.get('type', 'unknown'),
            "confidence": effect.get('confidence', 0.5),
            "parameters": effect.get('params', {{}})
        }})

    output = {{
        "effects": effects,
        "embeddings": []
    }}
    print(json.dumps(output))
except ImportError as e:
    print(json.dumps({{"error": f"Import failed: {{e}}"}}))
    sys.exit(1)
except Exception as e:
    print(json.dumps({{"error": str(e)}}))
    sys.exit(1)
"#,
            audio_path.display()
        );

        self.run_python_script(&script, audio_path)
    }

    fn run_python_script(
        &self,
        script: &str,
        audio_path: &Path,
    ) -> Result<EffectDetectionResult, EffectDetectionError> {
        let start = Instant::now();

        let python = get_python_path();
        let output = Command::new(&python)
            .args(["-c", script])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    EffectDetectionError::PythonNotFound
                } else {
                    EffectDetectionError::IoError(e)
                }
            })?;

        let processing_time = start.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(stderr = %stderr, "Python script failed");
            return Err(EffectDetectionError::ToolNotAvailable(
                self.tool.label().to_string(),
                Self::install_instructions(self.tool).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(stdout = %stdout, stderr = %stderr, "Python script output");

        // If stdout is empty or not valid JSON, include stderr in error
        if stdout.trim().is_empty() {
            let err_msg = if stderr.trim().is_empty() {
                "Python script produced no output".to_string()
            } else {
                format!("Python error: {}", stderr.trim())
            };
            return Err(EffectDetectionError::ExecutionFailed(err_msg));
        }

        let parsed: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
            // Include stderr in parse error for debugging
            let err_msg = if stderr.trim().is_empty() {
                format!("Failed to parse output: {}\nOutput was: {}", e, stdout.trim())
            } else {
                format!(
                    "Failed to parse output: {}\nstderr: {}",
                    e,
                    stderr.trim()
                )
            };
            EffectDetectionError::ParseError(err_msg)
        })?;

        if let Some(error) = parsed.get("error") {
            return Err(EffectDetectionError::ExecutionFailed(
                error.as_str().unwrap_or("Unknown error").to_string(),
            ));
        }

        let effects: Vec<DetectedEffect> = parsed
            .get("effects")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let name = v.get("name")?.as_str()?;
                        let confidence = v.get("confidence")?.as_f64()? as f32;
                        Some(DetectedEffect::new(name, confidence))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let embeddings: Option<Vec<f32>> = parsed
            .get("embeddings")
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect());

        // Get audio duration (approximate from file)
        let audio_duration_ms = crate::get_audio_duration_ms(audio_path).unwrap_or(0.0);

        Ok(EffectDetectionResult {
            tool: self.tool,
            effects,
            processing_time_ms: processing_time.as_secs_f64() * 1000.0,
            audio_duration_ms,
            embeddings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_labels() {
        assert_eq!(EffectDetectionTool::FxEncoderPlusPlus.label(), "Fx-Encoder++ (Sony)");
        assert_eq!(EffectDetectionTool::OpenAmp.label(), "OpenAmp");
        assert_eq!(EffectDetectionTool::Llm2FxTools.label(), "LLM2Fx-Tools");
    }
}
