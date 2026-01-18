# Audio Signal Analysis Architecture

Detailed architecture documentation for LlamaBurn's audio effect detection and analysis system.

## Overview

The audio signal analysis system detects and identifies audio effects applied to audio signals using ML-based detection tools, DSP heuristics, and optional LLM blind analysis.

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Audio     │────▶│   Effects   │────▶│  Detection  │────▶│   Results   │
│   Input     │     │    Rack     │     │   Service   │     │   Display   │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
     Dry                 Wet              ML + DSP           UI Report
```

## Control Flow

### Complete Pipeline

```
USER CLICKS "RECORD"
         │
         ▼
┌────────────────────────────────────┐
│  1. AUDIO ACQUISITION              │
│  ─────────────────────────────────│
│  • File: Load from disk            │
│  • Capture: Record from mic (16kHz)│
│  • Live: Stream 5s chunks          │
└────────────────────────────────────┘
         │
         ▼ (Capture mode only)
┌────────────────────────────────────┐
│  2. EFFECT CHAIN APPLICATION       │
│  ─────────────────────────────────│
│  dry_samples ──▶ EffectChain       │
│                      │             │
│                      ▼             │
│               wet_samples          │
│                      │             │
│  Extract ground truth (AppliedEffect)
└────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────┐
│  3. SAVE TEMP WAV FILES            │
│  ─────────────────────────────────│
│  Standard: /tmp/llamaburn_capture.wav
│  LLM2Fx:   /tmp/llamaburn_dry.wav  │
│            /tmp/llamaburn_wet.wav  │
└────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────┐
│  4. EFFECT DETECTION SERVICE       │
│  ─────────────────────────────────│
│  EffectDetectionService::detect()  │
│         │                          │
│         ├──▶ Fx-Encoder++ (Sony)   │
│         ├──▶ OpenAmp               │
│         └──▶ LLM2Fx-Tools ◀── dry+wet
└────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────┐
│  5. PYTHON ML EXECUTION            │
│  ─────────────────────────────────│
│  • Load audio via librosa/torch    │
│  • Run ML model (GPU if available) │
│  • Output JSON with detections     │
└────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────┐
│  6. SIGNAL ANALYSIS (LLM2Fx only)  │
│  ─────────────────────────────────│
│  • Cross-correlation (delay)       │
│  • Crest factor (compression)      │
│  • Spectral diff (EQ)              │
│  • Embedding distance              │
└────────────────────────────────────┘
         │
         ▼ (Optional)
┌────────────────────────────────────┐
│  7. LLM BLIND ANALYSIS             │
│  ─────────────────────────────────│
│  • Build prompt from measurements  │
│  • Call Ollama /api/generate       │
│  • Get natural language description│
└────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────┐
│  8. RESULT DISPLAY                 │
│  ─────────────────────────────────│
│  ┌───────────────────────────────┐ │
│  │ Ground Truth (Applied)       │ │
│  │ • Delay: 250ms, mix=0.5      │ │
│  ├───────────────────────────────┤ │
│  │ Detected Effects             │ │
│  │ • delay      [████░░] 78%    │ │
│  │ • echo       [██░░░░] 45%    │ │
│  ├───────────────────────────────┤ │
│  │ LLM Analysis                 │ │
│  │ "Sounds like a slapback..."  │ │
│  └───────────────────────────────┘ │
└────────────────────────────────────┘
```

## Components

### 1. Audio Input Sources

| Source | Sample Rate | Description |
|--------|-------------|-------------|
| **File** | Native | Load existing WAV/MP3 files |
| **Capture** | 16 kHz | Record from microphone for fixed duration |
| **Live** | 16 kHz | Continuous 5-second chunk analysis |

### 2. Effects Rack

Real-time audio processing chain with bypass capability.

```
Input ──▶ [Gain] ──▶ [HighPass] ──▶ [Delay] ──▶ [Reverb] ──▶ [Compressor] ──▶ Output
              │          │           │          │            │
              ▼          ▼           ▼          ▼            ▼
           Bypass     Bypass      Bypass     Bypass       Bypass
```

**Available Effects:**

| Effect | Parameters | Description |
|--------|------------|-------------|
| Delay | time_ms, feedback, mix | Echo/delay lines |
| Reverb | room_size, damping, mix | Spatial ambience |
| High Pass | cutoff_hz | Remove low frequencies |
| Low Pass | cutoff_hz | Remove high frequencies |
| Compressor | threshold, attack, release | Dynamic range control |
| Gain | gain_db | Volume adjustment |

### 3. Detection Tools

#### Fx-Encoder++ (Sony Research)
- **Input:** Single audio file
- **Output:** 32-dim embedding + effect predictions
- **Method:** Neural network trained on effect classification

#### OpenAmp
- **Input:** Single audio file
- **Output:** Effect predictions with confidence
- **Method:** Crowd-sourced amp/effect models

#### LLM2Fx-Tools
- **Input:** Dry + Wet audio pair
- **Output:** Effect predictions + signal analysis + embeddings
- **Method:** Comparative analysis with DSP heuristics

```
        Dry Audio                    Wet Audio
            │                            │
            ▼                            ▼
    ┌───────────────┐            ┌───────────────┐
    │  Fx-Encoder   │            │  Fx-Encoder   │
    │  Embedding    │            │  Embedding    │
    └───────┬───────┘            └───────┬───────┘
            │                            │
            └──────────┬─────────────────┘
                       ▼
              ┌───────────────┐
              │   Compare     │
              │ • Distance    │
              │ • Similarity  │
              │ • DSP Analysis│
              └───────────────┘
                       │
                       ▼
              Effect Predictions
```

### 4. Signal Analysis (DSP Heuristics)

| Analysis | Method | Detects |
|----------|--------|---------|
| Cross-correlation | Peak offset in correlation | Delay/Echo (>10ms) |
| Crest factor | Peak/RMS ratio change | Compression |
| Spectral diff | FFT magnitude comparison | EQ changes |
| RMS comparison | Energy level difference | Gain/Limiting |

### 5. LLM Blind Analysis

Optional natural language description using local Ollama models.

**Prompt Construction:**
```
Analyze this audio. Measurements:
- Embedding distance: 0.234
- Cosine similarity: 0.891
- Delay detected: 245ms
- Compression: crest factor changed by 1.2

Describe the audio effect applied.
```

## Data Structures

### EffectDetectionResult

```rust
pub struct EffectDetectionResult {
    // Detection metadata
    pub tool: EffectDetectionTool,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,

    // ML predictions
    pub effects: Vec<DetectedEffect>,
    pub embeddings: Option<Vec<f32>>,

    // Ground truth (from effects rack)
    pub applied_effects: Option<Vec<AppliedEffect>>,

    // DSP analysis
    pub signal_analysis: Option<SignalAnalysis>,
    pub embedding_distance: Option<f64>,
    pub cosine_similarity: Option<f64>,

    // LLM analysis
    pub llm_description: Option<String>,
    pub llm_model_used: Option<String>,
}
```

### DetectedEffect

```rust
pub struct DetectedEffect {
    pub name: String,           // "delay", "reverb", etc.
    pub confidence: f32,        // 0.0 - 1.0
    pub parameters: Option<HashMap<String, f32>>,
}
```

### AppliedEffect (Ground Truth)

```rust
pub struct AppliedEffect {
    pub name: String,
    pub parameters: HashMap<String, f32>,
    pub bypassed: bool,
}
```

### SignalAnalysis

```rust
pub struct SignalAnalysis {
    pub detected_delay_ms: Option<f64>,
    pub detected_reverb_rt60_ms: Option<f64>,
    pub frequency_change_db: Option<f64>,
    pub dynamic_range_change_db: Option<f64>,
    pub crest_factor_change: Option<f64>,
}
```

## File Structure

```
crates/
├── llamaburn-core/src/
│   └── audio.rs                 # Data types (EffectDetectionResult, etc.)
│
├── llamaburn-services/src/
│   ├── effect_detection.rs      # EffectDetectionService, Python execution
│   ├── audio_effects/
│   │   ├── mod.rs               # EffectChain, AudioEffect trait
│   │   └── native.rs            # Delay, Reverb, Compressor, etc.
│   ├── audio_input.rs           # Microphone capture (cpal)
│   └── audio_output.rs          # Playback with effects
│
└── llamaburn-gui/src/panels/
    └── benchmark.rs             # UI, entry points, result display
```

## Key Functions

| Function | Location | Purpose |
|----------|----------|---------|
| `start_effect_detection()` | benchmark.rs:1568 | File-based detection |
| `start_effect_detection_capture()` | benchmark.rs:1596 | Capture + detect |
| `start_effect_detection_live()` | benchmark.rs:1734 | Streaming analysis |
| `EffectDetectionService::detect()` | effect_detection.rs:115 | Main dispatcher |
| `detect_llm2fx()` | effect_detection.rs:259 | Dry/wet comparison |
| `run_python_script()` | effect_detection.rs:432 | Python subprocess |
| `get_llm_blind_analysis()` | effect_detection.rs:585 | Ollama integration |
| `EffectChain::process()` | audio_effects/mod.rs:79 | Apply effects |
| `get_applied_effects()` | audio_effects/mod.rs:126 | Extract ground truth |

## External Dependencies

### Python Environment
- Location: `~/.llamaburn/venv/bin/python`
- Packages: `torch`, `librosa`, `fxencoder_plusplus`, `openamp`

### Ollama (Optional)
- Endpoint: `http://localhost:11434/api/generate`
- Used for LLM blind analysis

### Temp Files
- Location: `/tmp/llamaburn_*.wav`
- Cleanup: Automatic after detection completes

## Error Handling

```rust
pub enum EffectDetectionError {
    PythonNotFound,                    // venv or python3 not found
    ToolNotAvailable(String, String),  // Tool name + install instructions
    ExecutionFailed(String),           // Python stderr output
    ParseError(String),                // JSON parse failure
    AudioNotFound(String),             // Audio file not found
    IoError(std::io::Error),
}
```

Errors propagate through the mpsc channel and display in the UI error field.
