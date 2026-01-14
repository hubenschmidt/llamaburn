# LlamaBurn

A benchmarking, profiling, and stress-testing suite for local LLM models.

- **Performance benchmarks** — TTFT, TPS, inter-token latency metrics
- **Stress testing** — Ramp, sweep, sustained, spike modes
- **Accuracy evaluation** — LLM-as-Judge using Claude or GPT
- **Local model support** — Auto-discovers Ollama models
- **Dual interface** — Standalone CLI + Leptos web UI

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

## Architecture

```
┌─────────────────────────────────────────────┐
│              CLI (llamaburn)                │
│           Rust binary, standalone           │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────┴──────────────────────────┐
│           Shared Core Crates                │
│  - benchmark, stress, eval, profiler        │
│  - multi-modal processors                   │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────┴──────────────────────────┐
│         Web UI (Leptos → WASM)              │
│  - Real-time charts                         │
│  - Model comparison                         │
│  - Export results                           │
└─────────────────────────────────────────────┘
```

## Project Structure

```
.
├── agent/
│   └── crates/
│       ├── llamaburn-core/       # Shared types, config
│       ├── llamaburn-benchmark/  # Performance benchmarks
│       ├── llamaburn-cli/        # CLI binary
│       └── llamaburn-web/        # Leptos frontend
├── spec/
│   └── SPEC.md                   # Full specification
└── docker-compose.yml
```

## Using Docker

Build and start the application:

```bash
docker compose up
```

The CLI starts automatically. Attach to the interactive CLI:

```bash
docker attach llamaburn-cli-1
```

Use the interactive commands:

```
help                    Show available commands
models, m               List available Ollama models
benchmark, b <model>    Run benchmark (e.g., `b llama3.1:8b -i 3`)
status, s               Show system status
clear                   Clear screen
exit, quit, q           Exit the application
```

## Prerequisites

- Docker & Docker Compose
- [Ollama](https://ollama.ai) running on host with models installed

```bash
ollama pull llama3.1:8b
ollama serve
```

## Native Build (Alternative)

```bash
cd agent
cargo build --release -p llamaburn-cli
./target/release/llamaburn
```

## Metrics

| Metric | Description |
|--------|-------------|
| **TTFT** | Time to first token (ms) |
| **TPS** | Tokens per second |
| **ITL** | Inter-token latency (ms) |
| **ISL** | Input sequence length |
| **OSL** | Output sequence length |

## License

MIT
