# LlamaBurn

A benchmarking, profiling, and stress-testing suite for local LLM models with audio analysis capabilities.

## Features

- **LLM Benchmarking** — TTFT, TPS, inter-token latency metrics
- **Stress Testing** — Ramp, sweep, sustained, spike modes
- **Accuracy Evaluation** — LLM-as-Judge using Claude or GPT
- **Audio Effect Analysis** — Detect and identify audio effects using ML models
- **Effects Rack** — Real-time audio processing with delay, reverb, EQ, compression
- **GPU Monitoring** — Real-time VRAM usage and GPU metrics
- **Local Model Support** — Auto-discovers Ollama models
- **Native GUI** — egui/eframe desktop application

![Capture Analyze](screenshot-capture-analyze.png)

![Effects Rack](screenshot-effects.png)

![LlamaBurn GUI](screenshot.png)

## Audio Effect Analysis

Analyze audio to detect and identify applied effects using ML-based detection tools.

### Signal Chain

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│  Audio Input    │────▶│   Effects Rack   │────▶│  Effect Detection   │
│  (Mic/File)     │     │  (Delay/Reverb/  │     │  (LLM2Fx-Tools)     │
│                 │     │   EQ/Compress)   │     │                     │
└─────────────────┘     └──────────────────┘     └─────────────────────┘
        │                        │                         │
        │ Dry Signal             │ Wet Signal              │
        └────────────────────────┼─────────────────────────┘
                                 ▼
                    ┌─────────────────────────┐
                    │    Combined Report      │
                    ├─────────────────────────┤
                    │ • Ground Truth (Rack)   │
                    │ • Detected Effects      │
                    │ • LLM Blind Analysis    │
                    └─────────────────────────┘
```

### Detection Tools

| Tool | Description |
|------|-------------|
| **LLM2Fx-Tools** | ML-based effect detection with dry+wet comparison |
| **Wav2Vec** | Audio embedding analysis |
| **Spectral** | Frequency domain analysis |

### Effects Rack

Built-in audio effects for signal chain testing:

| Effect | Parameters |
|--------|------------|
| **Delay** | Time (ms), Feedback, Mix |
| **Reverb** | Room Size, Damping, Mix |
| **High Pass** | Cutoff frequency (Hz) |
| **Low Pass** | Cutoff frequency (Hz) |
| **Compressor** | Threshold, Attack, Release |
| **Gain** | Level (dB) |

### Audio Modes

| Mode | Description |
|------|-------------|
| **File** | Analyze existing audio files |
| **Capture** | Record audio, apply effects, detect |
| **Live** | Real-time monitoring with effects |

## Audio Benchmarking

| Mode | Status | Description |
|------|--------|-------------|
| STT (Speech-to-Text) | ✅ Implemented | Whisper transcription with RTF metrics |
| Effect Detection | ✅ Implemented | ML-based audio effect identification |
| TTS (Text-to-Speech) | Planned | Voice synthesis benchmarking |
| Music Separation | Planned | Demucs stem isolation |
| Music Transcription | Planned | Basic Pitch note detection |
| Music Generation | Planned | AudioCraft/MusicGen |

### Building with Whisper (ROCm GPU)

```bash
# Install build deps
sudo apt install cmake clang

# Build with GPU acceleration (AMD ROCm)
cargo build -p llamaburn-gui --features whisper-gpu

# CPU-only build
cargo build -p llamaburn-gui --features whisper
```

Models download to `~/.local/share/llamaburn/whisper/`

### Benchmark Options

```bash
llamaburn benchmark <MODEL> [OPTIONS]

Arguments:
  <MODEL>    Model ID to benchmark (e.g., llama3.1:8b)

Options:
  -i, --iterations <N>    Number of iterations [default: 3]
  -w, --warmup <N>        Warmup runs [default: 1]
  -p, --prompts <FILE>    Prompts file (one per line)
  -t, --temperature <F>   Temperature [default: 0.7]
  -m, --max-tokens <N>    Max tokens to generate
  -o, --output <FILE>     Output JSON file
  --ollama-host <URL>     Ollama host [default: http://localhost:11434]
```

### Example Output

```
Model: llama3.1:8b

Iteration 1/3
  TTFT: 245.3 ms | TPS: 42.1 | Total: 1,523 ms

Iteration 2/3
  TTFT: 12.1 ms | TPS: 45.8 | Total: 1,412 ms

Iteration 3/3
  TTFT: 11.8 ms | TPS: 44.2 | Total: 1,456 ms

Summary:
  Avg TTFT: 89.7 ms
  Avg TPS: 44.0 (min: 42.1, max: 45.8)
  Avg Total: 1,463.7 ms
```

## CLI Usage

```bash
# Build the CLI
cd agent && cargo build --release -p llamaburn-cli

# List available models
llamaburn models

# Run benchmark
llamaburn benchmark llama3.1:8b --iterations 5

# Show system status
llamaburn status
```

### Commands

| Command | Description |
|---------|-------------|
| `models` | List available Ollama models |
| `benchmark` | Run benchmark tests on a model |
| `status` | Show system status |


## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         llamaburn-gui                               │
│                       (egui/eframe desktop)                         │
├─────────────────────────────────────────────────────────────────────┤
│  Panels: Benchmark │ Effects Rack │ GPU Monitor │ History │ Settings│
└────────────┬────────────────────────────────────────┬───────────────┘
             │                                        │
┌────────────┴────────────┐          ┌────────────────┴───────────────┐
│  llamaburn-benchmark    │          │       llamaburn-services       │
│  - Text/Audio runners   │          │  - OllamaClient (HTTP)         │
│  - Metrics collection   │          │  - WhisperService (STT)        │
└────────────┬────────────┘          │  - EffectDetection (LLM2Fx)    │
             │                       │  - AudioEffects (DSP chain)    │
             │                       │  - AudioInput/Output (cpal)    │
             │                       │  - GpuMonitor (nvidia-smi)     │
             │                       │  - HistoryService (SQLite)     │
             │                       └────────────────┬───────────────┘
┌────────────┴────────────────────────────────────────┴───────────────┐
│                         llamaburn-core                              │
│              Types, config, benchmark definitions                   │
└─────────────────────────────────────────────────────────────────────┘
             │                    │                    │
      ┌──────┴──────┐      ┌──────┴──────┐      ┌──────┴──────┐
      │   Ollama    │      │   Whisper   │      │  LLM2Fx     │
      │ (localhost) │      │ (whisper-rs)│      │  (Python)   │
      └─────────────┘      └─────────────┘      └─────────────┘
```

## Prerequisites

- [Ollama](https://ollama.ai) running with models installed

```bash
ollama pull llama3.1:8b
ollama serve
```

## Building the GUI

```bash
cd agent
cargo build --release -p llamaburn-gui
./target/release/llamaburn-gui
```

For development with hot reload:

```bash
cargo watch -x 'run -p llamaburn-gui'
```

## Metrics

| Metric | Description |
|--------|-------------|
| **TTFT** | Time to first token (ms) |
| **TPS** | Tokens per second |
| **ITL** | Inter-token latency (ms) |
| **ISL** | Input sequence length |
| **OSL** | Output sequence length |



## Troubleshooting

### Ollama connection issues

If models aren't loading, ensure Ollama is running:

```bash
curl http://localhost:11434/api/tags
```

### Ollama bound to localhost only

If running Ollama on a different machine, configure it to accept external connections:

```bash
sudo mkdir -p /etc/systemd/system/ollama.service.d
echo -e '[Service]\nEnvironment="OLLAMA_HOST=0.0.0.0"' | sudo tee /etc/systemd/system/ollama.service.d/override.conf
sudo systemctl daemon-reload && sudo systemctl restart ollama
```

Verify it's listening on all interfaces:

```bash
ss -tlnp | grep 11434
# Should show 0.0.0.0:11434 instead of 127.0.0.1:11434
```

## License

MIT
