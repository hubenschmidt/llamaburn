# LlamaBurn Specification

## Overview

LlamaBurn is a benchmarking, profiling, and evaluation suite for local LLM models. It provides actionable insights on model performance, system capacity, and response accuracy.

---

## Core Modules

### 1. Benchmark Runner

**Purpose**: Execute repeatable, standardized performance tests.

**Inputs**:
- Model ID (from Ollama discovery)
- Prompt set (predefined or custom)
- Iteration count
- Warm-up runs (discarded, default 2)
- Sampling config (temperature, top_p, top_k, max_tokens)

**Outputs** (per run):
- `time_to_first_token_ms` (TTFT) — latency to first token
- `inter_token_latency_ms` (ITL) — `(e2e - TTFT) / (tokens - 1)`
- `tokens_per_sec` (TPS) — generation throughput
- `total_generation_ms` — end-to-end latency
- `prompt_eval_ms` — input processing time
- `load_duration_ms` — cold vs warm start
- `input_sequence_length` (ISL) — input token count
- `output_sequence_length` (OSL) — generated token count
- `power_draw_watts` — GPU power during inference
- `energy_wh` — total energy consumed

**Test Modes**:
- **Standard**: Run prompts, collect metrics
- **Correctness**: Number-to-word conversion (validates output accuracy)

**Storage**: JSON file per run → `results/benchmarks/{model}_{timestamp}.json`

---

### 2. Stress Tester

**Purpose**: Determine capacity limits and failure thresholds.

**Test Modes**:
| Mode | Description |
|------|-------------|
| **Ramp** | Gradually increase concurrent requests until degradation |
| **Sweep** | Concurrency from 1 → max batch size, measure at each level |
| **Sustained** | Fixed load over duration (default 15 min), measure stability |
| **Spike** | Sudden load burst, measure impact and recovery time |

**Request Arrival Patterns**:
- `--arrival static` — constant interval between requests
- `--arrival poisson` — exponential distribution (realistic traffic)

**Metrics Captured**:
- Requests/sec at each concurrency level
- P50, P95, P99, P99.9 latency
- Error rate (timeouts, OOM, crashes)
- Degradation point (where latency > 2x baseline)
- Failure point (where errors > 5%)
- Recovery time (after spike, time to return to baseline)

**Capacity States**:
- **Idle**: No load, baseline memory/GPU usage
- **Optimal**: Max throughput with acceptable latency
- **Degraded**: Latency spike, throughput plateau
- **Failure**: Errors, crashes, OOM

**Options**:
- `--think-time` — delay between requests per client
- `--warmup-window` — exclude initial samples from stats
- `--cooldown-window` — exclude final samples from stats

**Storage**: `results/stress/{model}_{mode}_{timestamp}.json`

---

### 3. System Profiler

**Purpose**: Track hardware utilization during tests.

**Metrics**:
- CPU usage (%)
- RAM usage (MB)
- GPU utilization (%) — via `rocm-smi` (AMD)
- GPU VRAM (MB)
- Temperatures (optional)

**Collection**:
- Poll every 500ms during benchmark/stress runs
- Associate samples with request phases

**Storage**: Embedded in benchmark/stress result files under `system_profile[]`

---

### 4. Accuracy Evaluator

**Purpose**: Score model correctness using frontier model as judge (LLM-as-Judge).

**Workflow**:
```
1. Load eval set (questions + rubric + optional reference)
2. Send each question to local model (with or without web search)
3. Send {question, response, reference, rubric} to judge with CoT prompt
4. Judge returns score (1-5) + reasoning
5. Aggregate scores per criterion
```

**Evaluation Modes**:
- `--no-web-search` (default) — test pure model knowledge
- `--with-web-search` — test model's ability to use retrieved context
- `--pairwise --compare-to <model>` — head-to-head comparison

**Scoring** (1-5 integer scale for consistency):
| Score | Meaning |
|-------|---------|
| 1 | Completely wrong or unrelated |
| 2 | Partially correct but major errors |
| 3 | Mostly correct with minor issues |
| 4 | Correct with good detail |
| 5 | Perfect, comprehensive answer |

**Criteria** (evaluated separately):
- Accuracy — factual correctness
- Completeness — covers all aspects
- Coherence — logical and clear

**Eval Set Format** (`eval_sets/{name}.json`):
```json
{
  "name": "general_knowledge",
  "version": "1.0",
  "questions": [
    {
      "id": "q1",
      "prompt": "What is the capital of France?",
      "reference": "Paris",
      "category": "factual",
      "allow_web_search": false
    }
  ]
}
```

**Judge Providers** (configurable):
- Claude (Anthropic API) — uses CoT prompting
- OpenAI GPT-4/5

**Judge Config**:
- Temperature: 0.0 (deterministic)
- Chain-of-Thought prompting enabled

**Storage**: `results/evals/{model}_{evalset}_{timestamp}.json`

---

### 5. Prompt Sets

**Purpose**: Built-in prompt libraries for consistent benchmarking.

**Built-in Sets** (`prompts/`):
| Set | Description |
|-----|-------------|
| `default` | General mixed prompts |
| `coding` | Code generation tasks |
| `reasoning` | Math/logic problems |
| `factual` | Knowledge retrieval |
| `creative` | Open-ended generation |

**Custom Prompts**: Load from JSON file via `--prompts ./my_prompts.json`

---

### 6. Power & Cost Tracking

**Purpose**: Monitor energy consumption and estimate electricity costs.

**GPU Power Monitoring** (via `rocm-smi`):
- `power_draw_watts` — real-time power consumption
- `energy_wh` — total energy per run

**Cost Calculation**:
```
energy_wh = power_watts × duration_sec / 3600
cost = energy_wh / 1000 × kwh_rate
cost_per_1m_tokens = cost × (1_000_000 / tokens_generated)
```

**CLI**:
```bash
llamaburn benchmark --model llama3.1:q4 --kwh-rate 0.12
```

**Output**:
- Energy consumed (Wh)
- Cost per run ($)
- Cost per 1M tokens ($)
- Monthly estimate at sustained load

---

### 7. Multi-Modal Benchmarking

**Purpose**: Evaluate models across text, image, audio, video, and generative tasks.

**Supported Modalities**:

| Modality | Input | Output | Metrics |
|----------|-------|--------|---------|
| **Text** | Text prompt | Text generation | TTFT, TPS, accuracy |
| **Image** | PNG/JPG + prompt | Description/analysis | Accuracy, detail score |
| **Audio** | WAV/MP3 + prompt | Transcription/TTS | WER, latency, quality |
| **Video** | MP4 + prompt | Understanding/summary | Temporal accuracy |
| **3D Graphics** | Text prompt | WebGL/Three.js code | Render success, FPS |
| **Code Execution** | Text prompt | Runnable code | Correctness, runtime |

**Benchmark Types**:

*Text Generation* (primary):
- TTFT, ITL, TPS metrics
- Correctness tests
- Reasoning, coding, factual tasks

*Vision Understanding*:
- Image description accuracy
- Visual QA (VQA)
- OCR/document understanding

*Audio*:
- Speech-to-text (STT) — Word Error Rate (WER)
- Text-to-speech (TTS) — quality + latency
- Music/sound generation — quality + generation speed
- Audio event detection

---

### 8. Audio I/O System

**Purpose**: Capture, playback, and process audio for STT/TTS/music benchmarks.

**Audio Device Configuration**:
```toml
[audio]
input_device = "default"    # or specific device name
output_device = "default"
sample_rate = 44100
channels = 1                # mono for STT
buffer_size = 1024
```

**CLI Device Management**:
```bash
# List available audio devices
llamaburn audio list

# Test input (record 5 seconds)
llamaburn audio test-input --device "Blue Yeti" --duration 5s

# Test output (play test tone)
llamaburn audio test-output --device "Speakers"
```

**Capabilities**:

| Feature | CLI | Web UI | Implementation |
|---------|-----|--------|----------------|
| Device enumeration | `cpal` | Web Audio API | List input/output devices |
| Audio capture | `cpal` | `getUserMedia` | Mic → WAV buffer |
| Audio playback | `rodio` | Web Audio API | Stream/file → speakers |
| Waveform viz | N/A | `<canvas>` | Real-time visualization |
| File save | `hound` | Blob download | WAV/MP3 export |

**Benchmark Modes**:

*Live STT*:
```bash
# Real-time transcription from mic
llamaburn eval --model whisper:large --live-audio --device "default"
```

*TTS Generation*:
```bash
# Generate speech, stream + save
llamaburn eval --model tts-model --text "Hello world" --output speech.wav
```

*Music Generation*:
```bash
# Generate audio from prompt
llamaburn eval --model musicgen --prompt "upbeat jazz" --duration 30s --output music.wav
```

**Metrics**:
- `generation_time_ms` — time to produce audio
- `real_time_factor` — generation_time / audio_duration (< 1.0 = faster than real-time)
- `wer` — Word Error Rate for STT
- `audio_quality_score` — judge-evaluated (1-5)

**Rust Crates**:
- `cpal` — cross-platform audio I/O
- `rodio` — audio playback
- `hound` — WAV file I/O
- `rubato` — resampling

*Video*:
- Temporal reasoning
- Long-form understanding (1min → 1hr)
- Audio-visual joint reasoning

*Generative (Real-Time)*:
- 3D scene generation (renders in WebGPU sandbox)
- Audio waveform generation
- Code execution in isolated sandbox

**Storage**: `results/multimodal/{modality}/{model}_{timestamp}.json`

---

## Data Storage (No Database)

All results stored as JSON files:

```
results/
├── benchmarks/
│   └── llama3.1_q4_2024-01-13T10-30-00.json
├── stress/
│   └── llama3.1_q4_ramp_2024-01-13T11-00-00.json
├── evals/
│   └── llama3.1_q4_general_knowledge_2024-01-13T12-00-00.json
└── comparisons/
    └── comparison_2024-01-13T14-00-00.json
```

**Result File Structure**:
```json
{
  "model": { "id": "...", "name": "...", "quantization": "Q4_1" },
  "timestamp": "2024-01-13T10:30:00Z",
  "config": { /* run parameters */ },
  "results": { /* metrics */ },
  "system_profile": [ /* time-series samples */ ]
}
```

---

## API Endpoints (New)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/benchmark/run` | Start benchmark with config |
| POST | `/stress/run` | Start stress test |
| POST | `/eval/run` | Start accuracy evaluation |
| GET | `/results/{type}` | List results by type |
| GET | `/results/{type}/{id}` | Get specific result |
| POST | `/compare` | Compare multiple result files |
| GET | `/system/status` | Current CPU/GPU/RAM usage |

---

## Interfaces

LlamaBurn is accessible via **two interfaces** sharing the same backend:

### CLI (Terminal)

Binary: `llamaburn` (Rust, compiled from `agent/` workspace)

**Mode**: Standalone — runs directly against Ollama, no backend server required.

**Commands**:
```bash
# List available models
llamaburn models

# Benchmark
llamaburn benchmark --model llama3.1:q4 --iterations 10 --warmup 2
llamaburn benchmark --model llama3.1:q4 --prompts coding --temperature 0.0
llamaburn benchmark --model llama3.1:q4 --prompts ./my_prompts.json --max-tokens 512

# Stress test
llamaburn stress --model llama3.1:q4 --mode ramp --max-concurrency 20
llamaburn stress --model llama3.1:q4 --mode sweep --arrival poisson
llamaburn stress --model llama3.1:q4 --mode sustained --duration 15m

# Accuracy evaluation
llamaburn eval --model llama3.1:q4 --set general_knowledge --judge claude
llamaburn eval --model llama3.1:q4 --set factual --no-web-search
llamaburn eval --model llama3.1:q4 --set factual --with-web-search
llamaburn eval --model llama3.1:q4 --pairwise --compare-to mistral:7b --set coding

# Results & comparison
llamaburn results list benchmarks
llamaburn results show benchmarks/llama3.1_q4_2024-01-13.json
llamaburn compare results/benchmarks/*.json --output table
llamaburn compare results/benchmarks/*.json --output markdown > report.md

# System status & cost
llamaburn status
llamaburn benchmark --model llama3.1:q4 --kwh-rate 0.12

# Multi-modal evaluation
llamaburn eval --model llava:13b --set image_qa --input ./images/
llamaburn eval --model whisper:large --set audio_stt --input ./audio/
llamaburn eval --model video-llm --set video_mme --input ./videos/
llamaburn eval --model codegen --set threejs_gen --render
```

**Output Formats**:
- `--output table` (default) — human-readable tables
- `--output json` — machine-parseable JSON
- `--output csv` — for spreadsheet import

**Progress Display**:
- Live progress bar during runs
- Streaming metrics (tokens/sec, elapsed time)
- Final summary on completion

---

### Web UI (Leptos)

Full Rust frontend compiled to WASM, located in `llamaburn-web/` crate.

**Stack**:
- **UI Framework**: Leptos (Rust → WASM)
- **3D Rendering**: wgpu (WebGPU bindings)
- **Audio**: Web Audio API via wasm-bindgen
- **Code Sandbox**: iframe/Web Worker isolation

**Tabs**:

1. **Benchmark** — Configure and run benchmarks, view results
2. **Stress Test** — Configure load tests, live metrics graph
3. **Eval** — Select eval sets, view scores with judge reasoning
4. **Multi-Modal** — Image/audio/video/3D benchmarks with live preview
5. **Results** — List/compare past runs, export to CSV
6. **System Monitor** — Real-time CPU/GPU/RAM gauges

**High-Performance Features**:
- WebGPU canvas for 3D output rendering
- Web Audio API for audio waveform visualization
- Code sandbox for executing LLM-generated code

---

## Architecture

```
┌───────────────────────────────────────────────────┐
│                 CLI (llamaburn)                   │
│              Rust binary, standalone              │
└────────────────────────┬──────────────────────────┘
                         │
┌────────────────────────┴──────────────────────────┐
│              Shared Core Crates                   │
│  llamaburn-core, llamaburn-benchmark,             │
│  llamaburn-stress, llamaburn-eval,                │
│  llamaburn-profiler, llamaburn-multimodal         │
└────────────────────────┬──────────────────────────┘
                         │
┌────────────────────────┴──────────────────────────┐
│            Web UI (llamaburn-web)                 │
│               Leptos → WASM                       │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────┐  │
│  │    wgpu     │  │  Web Audio  │  │   Code    │  │
│  │  (WebGPU)   │  │     API     │  │  Sandbox  │  │
│  └─────────────┘  └─────────────┘  └───────────┘  │
└────────────────────────┬──────────────────────────┘
                         │
                         ▼
                 ┌───────────────┐
                 │    Ollama     │
                 └───────────────┘
```

**Project Structure** (replaces `client/`):
```
agent/crates/
├── llamaburn-core/       # Shared types, config
├── llamaburn-benchmark/  # Performance tests
├── llamaburn-stress/     # Load testing
├── llamaburn-eval/       # Accuracy evaluation
├── llamaburn-profiler/   # System metrics (rocm-smi)
├── llamaburn-multimodal/ # Image/audio/video processors
├── llamaburn-cli/        # CLI binary
└── llamaburn-web/        # Leptos frontend (WASM)
```

**Shared Logic**:
- All test logic in core crates, used by both CLI and Web
- **CLI**: Embeds runner directly (standalone, no server required)
- **Web UI**: Leptos WASM, served by Axum backend

---

## Configuration

**Environment Variables** (new):
```env
ANTHROPIC_API_KEY=...       # For Claude judge
OPENAI_API_KEY=...          # For GPT judge (existing)
LLAMABURN_RESULTS_DIR=./results
```

**Config File** (`llamaburn.toml` — optional):
```toml
[defaults]
judge_provider = "claude"
benchmark_iterations = 5
warmup_runs = 2
stress_duration_sec = 60
temperature = 0.0

[ollama]
host = "http://localhost:11434"

[cost]
kwh_rate = 0.12  # $/kWh (US average ~$0.12)
```

---

## Docker Usage

Build and run LlamaBurn via Docker:

```bash
# Build the image
docker compose build

# List available models
docker compose run --rm llamaburn models

# Run benchmark
docker compose run --rm llamaburn benchmark llama3.1:8b --iterations 5

# Stress test
docker compose run --rm llamaburn stress --model llama3.1:8b --mode ramp

# Results saved to ./results/
```

**Notes**:
- Ollama must be running on the host machine
- Results are persisted to `./results/` via volume mount
- Uses `host.docker.internal` to reach host Ollama

---

## Implementation Priority

1. **Phase 1**: Benchmark runner + JSON storage + CLI (`llamaburn benchmark`)
2. **Phase 2**: Stress tester with ramp mode + CLI (`llamaburn stress`)
3. **Phase 3**: Accuracy evaluator + CLI (`llamaburn eval`)
4. **Phase 4**: System profiler integration (rocm-smi)
5. **Phase 5**: Web UI dashboard (Benchmark/Stress/Eval tabs)
6. **Phase 6**: Comparison/reporting tools + results export

---

## Success Metrics

- Run 100-iteration benchmark in < 5 min for typical model
- Stress test identifies failure point within 10% accuracy
- Eval scores correlate with human judgment (spot check)
- Results reproducible across runs (< 5% variance)

---

## References

- [NVIDIA LLM Benchmarking Guide](https://developer.nvidia.com/blog/llm-benchmarking-fundamental-concepts/)
- [LLMPerf (Ray Project)](https://github.com/ray-project/llmperf)
- [GuideLLM (Red Hat)](https://developers.redhat.com/articles/2025/06/20/guidellm-evaluate-llm-deployments-real-world-inference)
- [LLM-as-Judge Best Practices](https://www.montecarlodata.com/blog-llm-as-judge/)
- [Evidently AI LLM Judge Guide](https://www.evidentlyai.com/llm-guide/llm-as-a-judge)
