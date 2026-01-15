# LlamaBurn Architecture

## Overview

Clean Architecture with Services layer - separates UI, business logic, and external integrations.

## Layer Diagram

```
┌─────────────────────────────────────────────────┐
│  UI Layer (llamaburn-gui)                       │
│  - egui panels (render state, capture intent)   │
│  - No direct I/O or business logic              │
└─────────────────────────────────────────────────┘
                      │
┌─────────────────────────────────────────────────┐
│  Services Layer (llamaburn-services)            │
│  - OllamaService (API calls)                    │
│  - GpuMonitorService (rocm-smi)                 │
│  - BenchmarkService (orchestration)             │
└─────────────────────────────────────────────────┘
                      │
┌─────────────────────────────────────────────────┐
│  Core Layer (llamaburn-core)                    │
│  - Domain types (BenchmarkResult, ModelConfig)  │
│  - Business logic (calculations, validation)    │
│  - No external dependencies                     │
└─────────────────────────────────────────────────┘
```

## Crate Structure

```
agent/crates/
├── llamaburn-core/       # Domain types, pure logic
├── llamaburn-services/   # External integrations
│   ├── ollama.rs         # Ollama API client
│   ├── gpu_monitor.rs    # rocm-smi integration
│   └── benchmark.rs      # Benchmark orchestration
├── llamaburn-gui/        # UI layer (egui)
│   ├── app.rs            # App state, routing
│   └── panels/           # UI components
└── llamaburn-benchmark/  # Benchmark algorithms
```

## Data Flow

1. UI captures user intent (button click, selection)
2. UI calls Service method via channel
3. Service performs I/O (HTTP, subprocess)
4. Service sends result back via channel
5. UI updates state and re-renders

## Service Traits

```rust
pub trait OllamaService {
    fn list_models(&self) -> Result<Vec<ModelConfig>>;
    fn generate(&self, req: GenerateRequest) -> Result<GenerateStream>;
}

pub trait GpuMonitorService {
    fn get_metrics(&self) -> Result<GpuMetrics>;
    fn subscribe(&self) -> Receiver<GpuMetrics>;
}

pub trait BenchmarkService {
    fn run(&self, config: BenchmarkConfig) -> Result<BenchmarkStream>;
    fn cancel(&self);
}
```

## Layer Responsibilities

### UI Layer (llamaburn-gui)
- Render current state using egui
- Capture user interactions
- Dispatch requests to services via channels
- Update state from service responses
- **No direct I/O** (HTTP, filesystem, subprocesses)

### Services Layer (llamaburn-services)
- Handle all external integrations
- Manage async operations
- Provide channel-based communication with UI
- Use config for connection details
- Return domain types from Core layer

### Core Layer (llamaburn-core)
- Define domain types (ModelConfig, BenchmarkResult, etc.)
- Pure business logic (calculations, validation)
- No external dependencies (only serde, thiserror)
- Shared across all other crates

## Dependency Direction

```
llamaburn-gui → llamaburn-services → llamaburn-core
                                  ↘
                         llamaburn-benchmark
```

- GUI depends on Services and Core
- Services depends on Core
- Core has no internal dependencies
- Benchmark algorithms in separate crate, used by Services
