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

use llamaburn_core::{DetectedEffect, EffectDetectionResult, EffectDetectionTool, SignalAnalysis};

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
                // Uses Fx-Encoder++ for dry/wet comparison
                "import fxencoder_plusplus; print('ok')"
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
                // Dry/wet comparison mode - uses Fx-Encoder++ under the hood
                "# Same as Fx-Encoder++ (used for dry/wet comparison)\n\
                 python3 -m venv ~/.llamaburn/venv\n\
                 source ~/.llamaburn/venv/bin/activate\n\
                 pip install fxencoder_plusplus"
            }
        }
    }

    /// Detect audio effects in the given audio file
    /// For LLM2Fx, reference_path (dry audio) is required
    pub fn detect(
        &self,
        audio_path: &Path,
        reference_path: Option<&Path>,
    ) -> Result<EffectDetectionResult, EffectDetectionError> {
        if !audio_path.exists() {
            return Err(EffectDetectionError::AudioNotFound(
                audio_path.display().to_string(),
            ));
        }

        if let Some(ref_path) = reference_path {
            if !ref_path.exists() {
                return Err(EffectDetectionError::AudioNotFound(
                    format!("Reference audio: {}", ref_path.display()),
                ));
            }
        }

        info!(tool = ?self.tool, path = %audio_path.display(), "Starting effect detection");
        let start = Instant::now();

        let result = match self.tool {
            EffectDetectionTool::FxEncoderPlusPlus => self.detect_fx_encoder(audio_path),
            EffectDetectionTool::OpenAmp => self.detect_openamp(audio_path),
            EffectDetectionTool::Llm2FxTools => self.detect_llm2fx(audio_path, reference_path),
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

    fn detect_llm2fx(
        &self,
        wet_path: &Path,
        dry_path: Option<&Path>,
    ) -> Result<EffectDetectionResult, EffectDetectionError> {
        let dry_path = dry_path.ok_or_else(|| {
            EffectDetectionError::ExecutionFailed(
                "LLM2Fx requires both dry (reference) and wet (processed) audio files".to_string(),
            )
        })?;

        // Use Fx-Encoder++ for embedding comparison + DSP signal analysis
        let script = format!(
            r#"
import json
import sys
import os
import io
import warnings
import numpy as np

warnings.filterwarnings('ignore')
os.environ['TRANSFORMERS_VERBOSITY'] = 'error'

def analyze_signal(dry_mono, wet_mono, sr):
    """DSP-based signal analysis to detect effect characteristics"""
    analysis = {{}}

    # Ensure same length
    min_len = min(len(dry_mono), len(wet_mono))
    dry = dry_mono[:min_len]
    wet = wet_mono[:min_len]

    # 1. Delay detection via cross-correlation
    # Look for echo peaks after the main signal
    correlation = np.correlate(wet, dry, mode='full')
    center = len(dry) - 1
    # Look for peaks after center (delayed copies)
    post_corr = correlation[center + int(sr * 0.01):]  # Skip first 10ms
    if len(post_corr) > 0:
        peak_idx = np.argmax(np.abs(post_corr))
        peak_val = np.abs(post_corr[peak_idx])
        main_peak = np.abs(correlation[center])
        if main_peak > 0 and peak_val / main_peak > 0.1:  # >10% of main
            delay_ms = (peak_idx + int(sr * 0.01)) / sr * 1000
            analysis['detected_delay_ms'] = float(delay_ms)

    # 2. Dynamic range / compression detection
    dry_rms = np.sqrt(np.mean(dry ** 2)) + 1e-10
    wet_rms = np.sqrt(np.mean(wet ** 2)) + 1e-10
    dry_peak = np.max(np.abs(dry)) + 1e-10
    wet_peak = np.max(np.abs(wet)) + 1e-10

    dry_crest = dry_peak / dry_rms
    wet_crest = wet_peak / wet_rms
    crest_change = wet_crest - dry_crest
    analysis['crest_factor_change'] = float(crest_change)

    # Dynamic range change in dB
    dr_change_db = 20 * np.log10(wet_rms / dry_rms)
    analysis['dynamic_range_change_db'] = float(dr_change_db)

    # 3. Frequency analysis for EQ detection
    n_fft = min(4096, len(dry))
    dry_spec = np.abs(np.fft.rfft(dry, n=n_fft))
    wet_spec = np.abs(np.fft.rfft(wet, n=n_fft))

    # Average spectral difference in dB
    spec_ratio = (wet_spec + 1e-10) / (dry_spec + 1e-10)
    freq_change_db = np.mean(20 * np.log10(spec_ratio))
    analysis['frequency_change_db'] = float(freq_change_db)

    return analysis

def main():
    import torch
    import librosa

    try:
        from fxencoder_plusplus import load_model
    except ImportError as e:
        return {{"error": f"fxencoder_plusplus not installed: {{e}}"}}

    device = 'cuda' if torch.cuda.is_available() else 'cpu'

    # Load dry and wet audio
    try:
        dry_wav, sr = librosa.load("{}", sr=44100, mono=False)
        wet_wav, _ = librosa.load("{}", sr=44100, mono=False)
    except Exception as e:
        return {{"error": f"Failed to load audio: {{e}}"}}

    # Convert to mono for signal analysis
    dry_mono = librosa.to_mono(dry_wav) if dry_wav.ndim > 1 else dry_wav
    wet_mono = librosa.to_mono(wet_wav) if wet_wav.ndim > 1 else wet_wav

    # DSP signal analysis
    signal_analysis = analyze_signal(dry_mono, wet_mono, sr)

    def to_stereo_tensor(wav):
        if wav.ndim == 1:
            t = torch.from_numpy(wav).unsqueeze(0).repeat(2, 1)
        else:
            t = torch.from_numpy(wav)
        return t.unsqueeze(0).to(device)

    dry_tensor = to_stereo_tensor(dry_wav)
    wet_tensor = to_stereo_tensor(wet_wav)

    # Load model (suppress stdout spam)
    try:
        old_stdout = sys.stdout
        sys.stdout = io.StringIO()
        model = load_model('default', device=device)
        sys.stdout = old_stdout
    except Exception as e:
        sys.stdout = old_stdout
        return {{"error": f"Failed to load model: {{e}}"}}

    # Get embeddings for both
    try:
        with torch.no_grad():
            dry_emb = model.get_fx_embedding(dry_tensor)
            wet_emb = model.get_fx_embedding(wet_tensor)

        diff = (wet_emb - dry_emb).cpu().detach().numpy().flatten()
        diff_norm = float((diff ** 2).sum() ** 0.5)

        dry_flat = dry_emb.cpu().detach().numpy().flatten()
        wet_flat = wet_emb.cpu().detach().numpy().flatten()
        cos_sim = float((dry_flat @ wet_flat) / (((dry_flat ** 2).sum() ** 0.5) * ((wet_flat ** 2).sum() ** 0.5) + 1e-8))

    except Exception as e:
        return {{"error": f"Failed to get embeddings: {{e}}"}}

    # Build detected effects list
    effects = []
    if signal_analysis.get('detected_delay_ms'):
        effects.append({{"name": "delay", "confidence": 0.9, "parameters": {{"time_ms": signal_analysis['detected_delay_ms']}}}})
    if abs(signal_analysis.get('crest_factor_change', 0)) > 1.0:
        effects.append({{"name": "compression", "confidence": min(abs(signal_analysis['crest_factor_change']) / 3.0, 1.0)}})
    if abs(signal_analysis.get('frequency_change_db', 0)) > 1.0:
        effects.append({{"name": "eq", "confidence": min(abs(signal_analysis['frequency_change_db']) / 6.0, 1.0)}})
    if diff_norm > 0.1:
        effects.append({{"name": "processing_detected", "confidence": min(diff_norm, 1.0)}})

    if not effects:
        effects.append({{"name": "minimal_difference", "confidence": 1.0}})

    return {{
        "effects": effects,
        "embeddings": diff.tolist()[:32],
        "dry_wet_comparison": True,
        "embedding_distance": diff_norm,
        "cosine_similarity": cos_sim,
        "signal_analysis": signal_analysis
    }}

try:
    result = main()
    print(json.dumps(result))
    sys.stdout.flush()
except Exception as e:
    print(json.dumps({{"error": f"Unexpected error: {{e}}"}}))
    sys.stdout.flush()
"#,
            dry_path.display(),
            wet_path.display()
        );

        self.run_python_script(&script, wet_path)
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

        // Parse extended fields
        let embedding_distance = parsed.get("embedding_distance").and_then(|v| v.as_f64());
        let cosine_similarity = parsed.get("cosine_similarity").and_then(|v| v.as_f64());

        // Parse signal analysis
        let signal_analysis = parsed.get("signal_analysis").map(|sa| {
            SignalAnalysis {
                detected_delay_ms: sa.get("detected_delay_ms").and_then(|v| v.as_f64()),
                detected_reverb_rt60_ms: sa.get("detected_reverb_rt60_ms").and_then(|v| v.as_f64()),
                frequency_change_db: sa.get("frequency_change_db").and_then(|v| v.as_f64()),
                dynamic_range_change_db: sa.get("dynamic_range_change_db").and_then(|v| v.as_f64()),
                crest_factor_change: sa.get("crest_factor_change").and_then(|v| v.as_f64()),
            }
        });

        // Get audio duration (approximate from file)
        let audio_duration_ms = crate::get_audio_duration_ms(audio_path).unwrap_or(0.0);

        Ok(EffectDetectionResult {
            tool: self.tool,
            effects,
            processing_time_ms: processing_time.as_secs_f64() * 1000.0,
            audio_duration_ms,
            embeddings,
            applied_effects: None,  // Set by caller with ground truth
            signal_analysis,
            llm_description: None,  // Set by caller after LLM call
            embedding_distance,
            cosine_similarity,
        })
    }
}

/// Generate LLM blind analysis prompt from detection results
pub fn build_llm_analysis_prompt(result: &EffectDetectionResult) -> String {
    let mut prompt = String::from(
        "Analyze this audio processing based on the measurements below. \
         Describe what audio effect(s) this sounds like WITHOUT knowing what was actually applied. \
         Focus on: delay/echo, reverb/space, EQ/tone changes, compression/dynamics.\n\n"
    );

    if let Some(distance) = result.embedding_distance {
        prompt.push_str(&format!("Embedding distance (0=identical): {:.3}\n", distance));
    }
    if let Some(similarity) = result.cosine_similarity {
        prompt.push_str(&format!("Cosine similarity (1=identical): {:.3}\n", similarity));
    }

    if let Some(ref sa) = result.signal_analysis {
        prompt.push_str("\nSignal Analysis:\n");
        if let Some(delay) = sa.detected_delay_ms {
            prompt.push_str(&format!("- Echo/delay detected at {:.0}ms\n", delay));
        }
        if let Some(dr) = sa.dynamic_range_change_db {
            prompt.push_str(&format!("- Dynamic range change: {:.1}dB\n", dr));
        }
        if let Some(crest) = sa.crest_factor_change {
            prompt.push_str(&format!("- Crest factor change: {:.2}\n", crest));
        }
        if let Some(freq) = sa.frequency_change_db {
            prompt.push_str(&format!("- Frequency change: {:.1}dB\n", freq));
        }
    }

    prompt.push_str("\nBased on these measurements, describe what processing was likely applied in 2-3 sentences.");
    prompt
}

/// Get LLM blind analysis using Ollama
pub fn get_llm_blind_analysis(
    result: &EffectDetectionResult,
    model: &str,
    ollama_host: &str,
) -> Result<String, EffectDetectionError> {
    let client = crate::OllamaClient::new(ollama_host);
    let prompt = build_llm_analysis_prompt(result);

    client.generate(model, &prompt).map_err(|e| {
        EffectDetectionError::ExecutionFailed(format!("LLM analysis failed: {}", e))
    })
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
