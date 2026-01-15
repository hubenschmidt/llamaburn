# Audio Benchmarking Specification

## Overview

### IMPORTANT! observe /rust-code-rules

Add comprehensive audio benchmarking to LlamaBurn with AMD ROCm GPU support for the 7900 XT (RDNA3, gfx1100).

### 6 Audio Benchmark Modes

| # | Mode | Description | Primary Tool | GPU |
|---|------|-------------|--------------|-----|
| 1 | **STT** | Speech-to-Text transcription | whisper-rs (hipblas) | ✅ |
| 2 | **TTS** | Text-to-Speech synthesis | Piper / F5-TTS | ✅/❌ |
| 3 | **Music Separation** | Split audio into stems | Demucs | ✅ |
| 4 | **Music Transcription** | Audio to MIDI | Basic Pitch | ✅ |
| 5 | **Music Generation** | Text prompt to audio | MusicGen | ✅ |
| 6 | **LLM Music Analysis** | Generate metadata about music | Qwen2-Audio | ✅ |

### Key Metric: Real-Time Factor (RTF)

```
RTF = processing_time / audio_duration
```
- RTF < 1.0 → Faster than real-time (good)
- RTF = 1.0 → Real-time
- RTF > 1.0 → Slower than real-time

---

## Implementation Phases Summary

| Phase | Scope | Modes | Dependencies |
|-------|-------|-------|--------------|
| **1** | Core Types + STT | STT | whisper-rs, hound, symphonia |
| **2** | TTS Benchmarking | TTS | Piper binary, F5-TTS (Python) |
| **3** | Music Analysis | Separation, Transcription | Demucs, Basic Pitch (Python) |
| **4** | Music Generation | Generation | MusicGen (Python) |
| **5** | LLM Audio | LLM Analysis | Qwen2-Audio / librosa+Ollama |

---

## ROCm-Compatible Options (Research Summary)

| Option | ROCm Support | Rust Native | Maturity | Recommendation |
|--------|--------------|-------------|----------|----------------|
| **whisper-rs (hipblas)** | ✅ Yes | ✅ Yes | Production | **Primary** |
| **PyTorch + Whisper** | ✅ Yes | ❌ Subprocess | Production | Fallback |
| **faster-whisper-rocm** | ✅ Yes | ❌ Service | Production | Alternative |
| **candle** | ❌ CUDA only | ✅ Yes | N/A | Not viable |
| **ort (ONNX)** | ⚠️ Deprecated | ✅ Yes | Declining | Not recommended |

### Primary: whisper-rs with hipblas

**Crate:** `whisper-rs = { version = "0.15", features = ["hipblas"] }`

- Native Rust bindings to whisper.cpp
- ROCm support via hipBLAS feature flag (Linux only)
- ~1 second processing per minute of audio
- Minimal dependencies

### Fallback: AMD Official Approach (PyTorch)

From [AMD ROCm Blog](https://rocm.blogs.amd.com/artificial-intelligence/whisper/README.html):

```python
import torch
from transformers import pipeline

device = "cuda:0" if torch.cuda.is_available() else "cpu"
pipe = pipeline("automatic-speech-recognition",
                model="openai/whisper-medium.en",
                chunk_length_s=30, device=device)
transcription = pipe("audio.wav")['text']
```

**Requirements:** ROCm 5.7+, PyTorch 2.2.1+

---

## Audio Benchmark Metrics

| Metric | Description | Formula |
|--------|-------------|---------|
| **RTF** | Real-Time Factor | `processing_time / audio_duration` |
| **Latency** | Time to first word | `first_word_timestamp` |
| **Total Time** | End-to-end processing | `start → transcription_complete` |
| **WER** | Word Error Rate (optional) | `(S + D + I) / N` where S=substitutions, D=deletions, I=insertions, N=words |

**RTF Interpretation:**
- RTF < 1.0 = Faster than real-time (good)
- RTF = 1.0 = Real-time
- RTF > 1.0 = Slower than real-time

---

## Implementation Plan

---

### Phase 1: Core Types + STT (Speech-to-Text)

**Goal:** Enable Whisper-based STT benchmarking with ROCm GPU acceleration.

#### 1.1 Core Audio Types

**File:** `crates/llamaburn-core/src/audio.rs` ✅ (created)

```rust
pub enum AudioMode { STT, TTS, MusicSeparation, MusicTranscription, MusicGeneration, LLMAnalysis }
pub enum WhisperModel { Tiny, Base, Small, Medium, Large, LargeV3 }
pub struct AudioBenchmarkConfig { ... }
pub struct AudioBenchmarkMetrics { ... }
pub struct AudioBenchmarkResult { ... }
```

**File:** `crates/llamaburn-core/src/lib.rs` ✅ (updated)
- Export audio module

#### 1.2 Whisper Service

**File:** `crates/llamaburn-services/src/whisper.rs` (CREATE)

```rust
pub struct WhisperService {
    model_path: PathBuf,
    context: Option<WhisperContext>,
}

impl WhisperService {
    pub fn new(model_dir: &Path) -> Self;
    pub fn load_model(&mut self, model: WhisperModel) -> Result<()>;
    pub fn transcribe(&self, audio_path: &Path) -> Result<TranscriptionResult>;
    pub fn transcribe_with_timing(&self, audio_path: &Path) -> Result<(TranscriptionResult, Duration)>;
}
```

#### 1.3 Audio Benchmark Runner

**File:** `crates/llamaburn-benchmark/src/audio_runner.rs` (CREATE)

```rust
pub struct AudioBenchmarkRunner { whisper: WhisperService }

pub enum AudioBenchmarkEvent {
    LoadingModel, ModelLoaded, Warmup, Iteration, Transcription, Done, Error
}
```

#### 1.4 GUI Integration (STT)

**File:** `crates/llamaburn-gui/src/panels/benchmark.rs` (MODIFY)

- Enable Audio tab in `BenchmarkType::is_implemented()`
- Add mode selector: `[STT] [TTS] [Music]`
- Add Whisper model selector
- Add file picker (rfd crate)
- Display RTF results + transcription preview

#### 1.5 Dependencies (Phase 1)

```toml
# llamaburn-services/Cargo.toml
whisper-rs = { version = "0.15", features = ["hipblas"] }
hound = "3.5"
symphonia = { version = "0.5", features = ["mp3", "flac", "aac"] }
rubato = "0.15"

# llamaburn-gui/Cargo.toml
rfd = "0.15"
```

---

### Phase 2: TTS (Text-to-Speech)

**Goal:** Add TTS benchmarking with Piper (fast CPU) and F5-TTS (GPU quality).

#### 2.1 TTS Types

**File:** `crates/llamaburn-core/src/audio.rs` (EXTEND)

```rust
pub enum TTSEngine { Piper, F5TTS, Bark, XTTS }
pub struct TTSBenchmarkConfig { engine, voice, text, iterations }
pub struct TTSBenchmarkMetrics { rtf, generation_time_ms, audio_duration_ms, chars_per_sec }
```

#### 2.2 TTS Service

**File:** `crates/llamaburn-services/src/tts.rs` (CREATE)

```rust
pub struct TTSService { engine: TTSEngine }

impl TTSService {
    pub fn generate_piper(text: &str, voice: &str, output: &Path) -> Result<Duration>;
    pub fn generate_pytorch(text: &str, model: &str, output: &Path) -> Result<Duration>;
}
```

#### 2.3 GUI (TTS Tab)

- Engine selector: `[Piper] [F5-TTS] [Bark] [XTTS]`
- Voice dropdown
- Text input area
- RTF + chars/sec results
- Audio playback button

#### 2.4 System Dependencies

```bash
# Piper binary
wget https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_amd64.tar.gz

# PyTorch TTS (optional)
pip install f5-tts bark TTS
```

---

### Phase 3: Music Analysis (Separation + Transcription)

**Goal:** Add Demucs source separation and Basic Pitch transcription.

#### 3.1 Music Types

**File:** `crates/llamaburn-core/src/audio.rs` (EXTEND)

```rust
pub enum MusicTask { SourceSeparation, Transcription, Generation, LLMAnalysis }
pub struct MusicBenchmarkConfig { task, audio_path, iterations }
pub struct MusicBenchmarkMetrics { rtf, stems_extracted, notes_detected }
```

#### 3.2 Music Service

**File:** `crates/llamaburn-services/src/music.rs` (CREATE)

```rust
pub struct MusicService;

impl MusicService {
    pub fn separate_demucs(input: &Path, output_dir: &Path) -> Result<(Duration, Vec<PathBuf>)>;
    pub fn transcribe_basicpitch(input: &Path, output: &Path) -> Result<(Duration, u32)>;
}
```

#### 3.3 GUI (Music Tab)

- Task selector: `[Separation] [Transcription] [Generation] [LLM Analysis]`
- Audio file picker
- Stem playback buttons (for separation)
- MIDI download (for transcription)

#### 3.4 System Dependencies

```bash
pip install demucs basic-pitch
```

---

### Phase 4: Music Generation

**Goal:** Add MusicGen text-to-music generation.

#### 4.1 Generation Types

```rust
pub struct MusicGenConfig { prompt, duration_sec, model_size }
pub struct MusicGenMetrics { rtf, audio_path }
```

#### 4.2 Generation Service

**File:** `crates/llamaburn-services/src/music.rs` (EXTEND)

```rust
impl MusicService {
    pub fn generate_musicgen(prompt: &str, duration: u32, model: &str, output: &Path) -> Result<Duration>;
}
```

#### 4.3 GUI (Generation)

- Text prompt input
- Duration slider (1-30 sec)
- Model selector: `[Small] [Medium] [Large]`
- Audio playback

#### 4.4 System Dependencies

```bash
pip install audiocraft
```

---

### Phase 5: LLM Music Analysis

**Goal:** Use audio-understanding LLMs to generate rich music metadata.

#### 5.1 LLM Analysis Types

```rust
pub struct LLMAnalysisConfig { audio_path, model, prompt }
pub struct LLMAnalysisMetrics { processing_time_ms, tokens_generated, description }
```

#### 5.2 Analysis Service

**File:** `crates/llamaburn-services/src/music.rs` (EXTEND)

```rust
impl MusicService {
    // Primary: Qwen2-Audio
    pub fn analyze_qwen2audio(input: &Path, prompt: &str) -> Result<(Duration, String)>;

    // Fallback: librosa + Ollama
    pub fn analyze_hybrid(input: &Path, llm_model: &str) -> Result<(Duration, String)>;
}
```

#### 5.3 GUI (LLM Analysis)

- Audio file picker
- Model selector: `[Qwen2-Audio-7B] [Hybrid (librosa+Ollama)]`
- Custom prompt input
- Description output with copy button

#### 5.4 System Dependencies

```bash
# Primary
pip install transformers accelerate

# Fallback
pip install librosa
# + Ollama running locally
```

---

## Dependencies Summary

**Cargo.toml (llamaburn-services):**
```toml
# Audio transcription
whisper-rs = { version = "0.15", features = ["hipblas"] }
hound = "3.5"           # WAV file I/O
symphonia = "0.5"       # Audio decoding (MP3, FLAC, etc.)
rubato = "0.15"         # Resampling to 16kHz
```

**Cargo.toml (llamaburn-gui):**
```toml
rfd = "0.15"            # Native file dialogs
```

**System Requirements:**
- ROCm 5.7+ installed
- hipBLAS libraries available
- FFmpeg (optional, for format conversion)

---

## Model Management

### Download Locations

Models downloaded from HuggingFace to:
```
~/.local/share/llamaburn/whisper/
├── ggml-tiny.bin       (~75 MB)
├── ggml-base.bin       (~142 MB)
├── ggml-small.bin      (~466 MB)
├── ggml-medium.bin     (~1.5 GB)
└── ggml-large-v3.bin   (~3.1 GB)
```

### Download URLs (ggml format)

```
https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{size}.bin
```

---

## History Storage

Extend `BenchmarkHistoryEntry` to support audio:

```rust
pub struct BenchmarkHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub model_id: String,           // For text: "llama3:8b", for audio: "whisper-medium"
    pub summary: BenchmarkSummary,  // Unified summary with avg_tps OR avg_rtf
    // ...
}
```

For audio benchmarks:
- `model_id` = "whisper-{size}" (e.g., "whisper-medium")
- `summary.avg_tps` reused as RTF (or add `avg_rtf` field)

---

## Audio Format Support

| Format | Extension | Support |
|--------|-----------|---------|
| WAV | .wav | Native (hound) |
| MP3 | .mp3 | Via symphonia |
| FLAC | .flac | Via symphonia |
| M4A/AAC | .m4a | Via symphonia |
| OGG | .ogg | Via symphonia |

All formats converted to 16kHz mono WAV before processing (Whisper requirement).

---

## Verification

1. **Enable Audio tab** → Audio tab becomes clickable
2. **Select model** → Model selector shows Tiny/Base/Small/Medium/Large
3. **Select audio file** → File dialog opens, shows duration after selection
4. **Run benchmark** → Progress shows, RTF displayed per iteration
5. **View results** → RTF, processing time, transcription preview shown
6. **Check history** → Audio benchmarks appear in History tab
7. **Compare** → Audio benchmarks can be compared (by RTF)

---

## Text-to-Speech (TTS) Benchmarking

### TTS Options Summary

| Engine | Type | ROCm/GPU | Speed | Quality | Integration |
|--------|------|----------|-------|---------|-------------|
| **Piper** | ONNX/CPU | ❌ CPU only | ⚡ Very Fast | Good | Subprocess |
| **Bark** | PyTorch | ✅ ROCm | 🐢 Slow | Excellent | Subprocess |
| **Coqui XTTS** | PyTorch | ✅ ROCm | Medium | Excellent | Subprocess |
| **F5-TTS** | PyTorch | ✅ ROCm | Fast | Great | Subprocess |
| **StyleTTS2** | PyTorch | ✅ ROCm | Medium | Best | Subprocess |

### Primary: Piper (Fast CPU)

**What:** Lightweight, fast TTS using ONNX models. Optimized for speed.

**Installation:**
```bash
# Download Piper binary
wget https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_amd64.tar.gz
tar -xzf piper_amd64.tar.gz

# Download voice model (e.g., en_US-lessac-medium)
wget https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx
wget https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json
```

**Usage from Rust:**
```rust
use std::process::Command;

fn generate_speech_piper(text: &str, output_path: &Path, voice: &str) -> Result<Duration> {
    let start = Instant::now();

    Command::new("piper")
        .args(["--model", voice, "--output_file", output_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .spawn()?
        .stdin.unwrap()
        .write_all(text.as_bytes())?;

    Ok(start.elapsed())
}
```

**Voices:** 100+ voices in 30+ languages
**Speed:** ~50x real-time on CPU (very fast)
**Quality:** Good for general use, natural sounding

### Secondary: PyTorch TTS (GPU Quality)

**What:** High-quality neural TTS models running on ROCm GPU.

**Recommended Models:**

1. **F5-TTS** (Best balance of speed/quality)
```python
# Install
pip install f5-tts

# Usage
from f5_tts import F5TTS
tts = F5TTS()
audio = tts.generate("Hello world", speaker="default")
```

2. **Bark** (Most expressive, supports emotions/music)
```python
from bark import SAMPLE_RATE, generate_audio, preload_models
preload_models()
audio = generate_audio("Hello! [laughs] How are you?")
```

3. **Coqui XTTS** (Multi-lingual, voice cloning)
```python
from TTS.api import TTS
tts = TTS("tts_models/multilingual/multi-dataset/xtts_v2")
tts.tts_to_file(text="Hello", file_path="output.wav", language="en")
```

**Rust Integration (subprocess):**
```rust
fn generate_speech_pytorch(text: &str, output_path: &Path, model: &str) -> Result<Duration> {
    let start = Instant::now();

    let script = format!(r#"
from f5_tts import F5TTS
import soundfile as sf
tts = F5TTS()
audio = tts.generate("{}")
sf.write("{}", audio, 24000)
"#, text.replace("\"", "\\\""), output_path.display());

    Command::new("python3")
        .args(["-c", &script])
        .status()?;

    Ok(start.elapsed())
}
```

### TTS Metrics

| Metric | Description | Formula |
|--------|-------------|---------|
| **RTF** | Real-Time Factor | `generation_time / audio_duration` |
| **Latency** | Time to first audio byte | `start → first_sample` |
| **Characters/sec** | Generation throughput | `char_count / generation_time` |
| **Audio Quality** | Subjective score (future) | MOS 1-5 via judge |

### TTS Data Structures

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TTSEngine {
    #[default]
    Piper,      // Fast CPU
    F5TTS,      // GPU balanced
    Bark,       // GPU expressive
    XTTS,       // GPU multilingual
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSBenchmarkConfig {
    pub engine: TTSEngine,
    pub voice: String,
    pub text: String,
    pub iterations: u32,
    pub warmup_runs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSBenchmarkMetrics {
    pub real_time_factor: f64,
    pub generation_time_ms: f64,
    pub audio_duration_ms: f64,
    pub characters: u32,
    pub chars_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSBenchmarkSummary {
    pub avg_rtf: f64,
    pub min_rtf: f64,
    pub max_rtf: f64,
    pub avg_chars_per_sec: f64,
    pub iterations: u32,
}
```

### TTS UI Flow

```
Audio Benchmark
───────────────
Mode: [STT] [TTS]
           ↓ (selected)

TTS Benchmark
───────────────
Engine:   [Piper ▼] [F5-TTS] [Bark] [XTTS]
Voice:    [en_US-lessac-medium ▼]
Text:     [Enter text to synthesize...]
          "The quick brown fox jumps over the lazy dog."
Iterations: [5]

[Run TTS Benchmark]

Results
───────────────
Avg RTF: 0.02x (50x faster than real-time)
Avg Time: 45ms for 2.3s audio
Speed: 890 chars/sec
[▶ Play Generated Audio]
```

### TTS Model Storage

```
~/.local/share/llamaburn/tts/
├── piper/
│   ├── en_US-lessac-medium.onnx      (~60 MB)
│   └── en_US-lessac-medium.onnx.json
├── f5-tts/
│   └── (auto-downloaded by library)
└── bark/
    └── (auto-downloaded by library)
```

### TTS Verification

1. **Select TTS mode** → TTS config UI appears
2. **Select engine** → Piper (fast) or PyTorch models (quality)
3. **Enter text** → Default sample text provided
4. **Run benchmark** → Progress shows, RTF per iteration
5. **View results** → RTF, chars/sec, audio duration
6. **Play audio** → Listen to generated sample
7. **Check history** → TTS benchmarks saved with engine info

---

## Music Analysis Benchmarking

### Music Analysis Options

| Capability | Tool/Model | ROCm/GPU | Speed | Integration |
|------------|------------|----------|-------|-------------|
| **Source Separation** | Demucs (Meta) | ✅ PyTorch | Medium | Subprocess |
| **Music Transcription** | Basic Pitch | ✅ PyTorch | Fast | Subprocess |
| **Beat Detection** | madmom | ❌ CPU | Fast | Python lib |
| **Chord Detection** | autochord | ❌ CPU | Fast | Python lib |
| **Genre Classification** | Custom models | ✅ PyTorch | Fast | Subprocess |
| **Music Generation** | MusicGen | ✅ PyTorch | Slow | Subprocess |

### Primary: Demucs (Source Separation)

**What:** Separate audio into stems: vocals, drums, bass, other.
GPU-intensive, good benchmark for audio processing.

**Installation:**
```bash
pip install demucs
```

**Usage:**
```python
import demucs.separate
import torch

# Separate audio into stems
demucs.separate.main(["--two-stems", "vocals", "-n", "htdemucs", "song.mp3"])
# Outputs: song/htdemucs/vocals.wav, song/htdemucs/no_vocals.wav
```

**Rust Integration:**
```rust
fn separate_audio(input: &Path, output_dir: &Path) -> Result<Duration> {
    let start = Instant::now();

    Command::new("python3")
        .args(["-m", "demucs", "--two-stems", "vocals",
               "-o", output_dir.to_str().unwrap(),
               input.to_str().unwrap()])
        .status()?;

    Ok(start.elapsed())
}
```

**Metrics:**
- RTF (processing_time / audio_duration)
- Stems extracted (vocals, drums, bass, other)
- GPU memory usage

### Secondary: Basic Pitch (Music Transcription)

**What:** Convert audio to MIDI notes. Developed by Spotify.

**Installation:**
```bash
pip install basic-pitch
```

**Usage:**
```python
from basic_pitch.inference import predict
from basic_pitch import ICASSP_2022_MODEL_PATH

model_output, midi_data, note_events = predict(
    "audio.wav",
    ICASSP_2022_MODEL_PATH
)
midi_data.write("output.mid")
```

**Metrics:**
- Notes detected
- Processing time
- Pitch accuracy (if ground truth available)

### Tertiary: MusicGen (Music Generation)

**What:** Generate music from text prompts. Meta's audio generation model.

**Installation:**
```bash
pip install audiocraft
```

**Usage:**
```python
from audiocraft.models import MusicGen
from audiocraft.data.audio import audio_write

model = MusicGen.get_pretrained('facebook/musicgen-medium')
model.set_generation_params(duration=10)  # 10 seconds

descriptions = ["upbeat jazz with saxophone"]
wav = model.generate(descriptions)

audio_write("output", wav[0].cpu(), model.sample_rate)
```

**Models:**
- `musicgen-small` (~300MB) - Fast, lower quality
- `musicgen-medium` (~1.5GB) - Balanced
- `musicgen-large` (~3.5GB) - Best quality

**Metrics:**
- Generation RTF
- Audio duration generated
- Prompt complexity

### Quaternary: LLM Audio Analysis (Metadata Generation)

**What:** Use audio-understanding LLMs to generate rich metadata about music.

**Audio-Capable LLMs:**

| Model | ROCm Support | Capabilities |
|-------|--------------|--------------|
| **Qwen2-Audio** | ✅ PyTorch | Audio understanding, description |
| **SALMONN** | ✅ PyTorch | Speech + audio + music understanding |
| **LTU** | ✅ PyTorch | Listen, Think, Understand |
| **Gemini** | ❌ API only | Audio analysis via API |

**Primary: Qwen2-Audio**

```python
from transformers import Qwen2AudioForConditionalGeneration, AutoProcessor

model = Qwen2AudioForConditionalGeneration.from_pretrained(
    "Qwen/Qwen2-Audio-7B-Instruct",
    torch_dtype=torch.float16,
    device_map="auto"
)
processor = AutoProcessor.from_pretrained("Qwen/Qwen2-Audio-7B-Instruct")

# Analyze music
conversation = [
    {"role": "user", "content": [
        {"type": "audio", "audio_url": "music.wav"},
        {"type": "text", "text": "Describe this music in detail: genre, mood, instruments, tempo, and style."}
    ]}
]

inputs = processor.apply_chat_template(conversation, return_tensors="pt")
outputs = model.generate(**inputs, max_new_tokens=500)
description = processor.decode(outputs[0], skip_special_tokens=True)
```

**Example Output:**
```
This is an upbeat jazz track featuring:
- Genre: Contemporary jazz with funk influences
- Tempo: ~120 BPM, medium-fast groove
- Instruments: Alto saxophone (lead), piano comping, upright bass, drums with brushes
- Mood: Energetic, optimistic, sophisticated
- Style: Reminiscent of 1960s hard bop with modern production
- Key: Bb major with ii-V-I progressions
```

**Alternative: Hybrid Approach (Feature Extraction + Ollama)**

For systems without audio LLMs, extract features then describe via text LLM:

```python
import librosa
import json

def extract_music_features(audio_path):
    y, sr = librosa.load(audio_path)

    return {
        "tempo": float(librosa.beat.tempo(y=y, sr=sr)[0]),
        "key": librosa.key_to_notes(librosa.estimate_tuning(y=y, sr=sr)),
        "duration_sec": float(len(y) / sr),
        "energy": float(librosa.feature.rms(y=y).mean()),
        "spectral_centroid": float(librosa.feature.spectral_centroid(y=y, sr=sr).mean()),
        "zero_crossing_rate": float(librosa.feature.zero_crossing_rate(y).mean()),
    }

# Then send to Ollama
features = extract_music_features("music.wav")
prompt = f"""Analyze this music based on these audio features:
{json.dumps(features, indent=2)}

Describe the likely genre, mood, and characteristics."""

# Call Ollama API with prompt
```

**Metrics for LLM Analysis:**
- Processing time (audio encoding + LLM generation)
- Token throughput
- Description quality (judge-evaluated)

### Music Analysis Data Structures

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MusicTask {
    #[default]
    SourceSeparation,  // Demucs - split into stems
    Transcription,     // Basic Pitch - audio to MIDI
    BeatDetection,     // madmom - BPM, beats
    ChordDetection,    // autochord - chord progressions
    Generation,        // MusicGen - text to music
    LLMAnalysis,       // Qwen2-Audio - describe music
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicBenchmarkConfig {
    pub task: MusicTask,
    pub audio_path: Option<PathBuf>,   // For analysis tasks
    pub prompt: Option<String>,         // For generation / LLM analysis prompt
    pub duration_sec: Option<u32>,      // For generation
    pub llm_model: Option<String>,      // For LLM analysis (e.g., "Qwen/Qwen2-Audio-7B")
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicBenchmarkMetrics {
    pub real_time_factor: f64,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    // Task-specific
    pub stems_extracted: Option<Vec<String>>,  // Demucs
    pub notes_detected: Option<u32>,           // Basic Pitch
    pub bpm_detected: Option<f64>,             // Beat detection
    pub chords_detected: Option<Vec<String>>,  // Chord detection
    pub generated_audio_path: Option<PathBuf>, // MusicGen
    pub llm_description: Option<String>,       // LLM Analysis
    pub tokens_generated: Option<u32>,         // LLM Analysis
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicBenchmarkSummary {
    pub avg_rtf: f64,
    pub min_rtf: f64,
    pub max_rtf: f64,
    pub iterations: u32,
}
```

### Music Analysis UI Flow

```
Audio Benchmark
───────────────
Mode: [STT] [TTS] [Music]
                   ↓ (selected)

Music Analysis
───────────────
Task:     [Source Separation ▼] [Transcription] [Generation]
Audio:    [Select File...] song.mp3 (3:24)

[Run Music Benchmark]

Results (Source Separation)
───────────────
RTF: 0.85x (near real-time)
Time: 174s for 204s audio
Stems: vocals.wav, drums.wav, bass.wav, other.wav
[▶ Play Vocals] [▶ Play Drums] [▶ Play Bass]
```

```
Music Generation
───────────────
Task:     [Generation]
Prompt:   [Enter music description...]
          "ambient electronic with soft synths and reverb"
Duration: [10] seconds
Iterations: [3]

[Run Generation Benchmark]

Results
───────────────
RTF: 2.5x (slower than real-time)
Time: 25s for 10s audio
[▶ Play Generated Audio]
```

```
LLM Music Analysis
───────────────
Task:     [LLM Analysis]
Audio:    [Select File...] jazz_track.mp3 (3:24)
Model:    [Qwen2-Audio-7B ▼]
Prompt:   [Describe this music...]
          "Describe genre, mood, instruments, tempo, and style"
Iterations: [3]

[Run Analysis Benchmark]

Results
───────────────
Avg Time: 2.3s
Tokens: 245 @ 106 TPS

Description:
"This is an upbeat jazz track featuring alto saxophone
as the lead instrument, accompanied by piano comping,
upright bass, and drums with brushes. The tempo is
approximately 120 BPM with a medium-fast groove..."

[Copy Description]
```

### Music Model Storage

```
~/.local/share/llamaburn/music/
├── demucs/
│   └── htdemucs/           (auto-downloaded, ~300MB)
├── basic-pitch/
│   └── (auto-downloaded)
└── musicgen/
    └── (auto-downloaded, ~3.5GB for medium)
```

### Music Analysis Verification

1. **Select Music mode** → Music task selector appears
2. **Source Separation** → Select audio, run Demucs, play stems
3. **Transcription** → Select audio, get MIDI output, view notes
4. **Generation** → Enter prompt, generate audio, play result
5. **View metrics** → RTF, processing time displayed
6. **History** → Music benchmarks saved

---

## Audio Benchmark Modes Summary

| # | Mode | Input | Output | Key Metrics | Tool | GPU |
|---|------|-------|--------|-------------|------|-----|
| 1 | **STT** | Audio file | Text | RTF, WER | whisper-rs | ✅ |
| 2 | **TTS** | Text | Audio file | RTF, chars/sec | Piper/F5-TTS | ✅/❌ |
| 3 | **Separation** | Audio file | Stem files (4) | RTF | Demucs | ✅ |
| 4 | **Transcription** | Audio file | MIDI | RTF, notes | Basic Pitch | ✅ |
| 5 | **Generation** | Text prompt | Audio file | RTF | MusicGen | ✅ |
| 6 | **LLM Analysis** | Audio file | Text description | TPS, quality | Qwen2-Audio | ✅ |

### Quick Start (Phase 1 - STT only)

```bash
# 1. Install ROCm + hipBLAS
# 2. Add dependencies to Cargo.toml
# 3. Build with: cargo build --release
# 4. Run: ./target/release/llamaburn-gui
# 5. Click Audio tab → Select file → Run
```

---

## Future Enhancements

- **WER Calculation**: Compare STT against ground truth
- **Live Microphone**: Real-time STT benchmarking
- **Voice Cloning**: Benchmark XTTS voice cloning
- **Streaming TTS**: Measure time-to-first-audio
- **Quality Scoring**: MOS evaluation via LLM judge
- **Batch Processing**: Multiple files/texts
- **Audio Fingerprinting**: Identify songs
- **Emotion Detection**: Analyze mood in audio

---

## References

**STT:**
- [AMD ROCm Blog: Speech-to-Text with Whisper](https://rocm.blogs.amd.com/artificial-intelligence/whisper/README.html)
- [whisper-rs crate](https://crates.io/crates/whisper-rs)
- [whisper.cpp ROCm support](https://github.com/ggml-org/whisper.cpp)
- [ggml Whisper models](https://huggingface.co/ggerganov/whisper.cpp)

**TTS:**
- [Piper TTS](https://github.com/rhasspy/piper)
- [Piper Voices](https://huggingface.co/rhasspy/piper-voices)
- [F5-TTS](https://github.com/SWivid/F5-TTS)
- [Bark (Suno)](https://github.com/suno-ai/bark)
- [Coqui XTTS](https://github.com/coqui-ai/TTS)

**Music:**
- [Demucs (Meta)](https://github.com/facebookresearch/demucs)
- [Basic Pitch (Spotify)](https://github.com/spotify/basic-pitch)
- [MusicGen (Meta)](https://github.com/facebookresearch/audiocraft)
- [madmom](https://github.com/CPJKU/madmom)

**Audio LLMs:**
- [Qwen2-Audio](https://huggingface.co/Qwen/Qwen2-Audio-7B-Instruct)
- [SALMONN](https://github.com/bytedance/SALMONN)
- [librosa](https://librosa.org/)
