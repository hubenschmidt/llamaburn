# Benchmark Enhancements Spec

## Overview

Add features to fill the empty screen real estate and support multi-modal benchmarking:

1. **Benchmark Types** - Support Text, Vision, and Audio modalities
2. **Live Output** - Stream model responses during benchmark runs
3. **Benchmark History** - Persist results, show trends over time
4. **Model Comparison** - Side-by-side comparison of multiple runs

---

## 1. Benchmark Types (Multi-Modal)

### Supported Modalities (6 Types)

| Type | Input | Output | Metrics |
|------|-------|--------|---------|
| **Text** | Text prompt | Text response | TPS, TTFT, ITL |
| **Image** | Image + prompt | Text description | TTFT, TPS, image proc time |
| **Audio** | Audio file | Transcription | RTF, WER, latency |
| **Video** | Video file + prompt | Description/analysis | Frame rate, TTFT, TPS |
| **3D Graphics** | 3D model/scene | Render/description | Render time, TPS |
| **Code** | Code prompt | Code output | TPS, TTFT, syntax validity |

### Data Structures

```rust
#[derive(Clone, Serialize, Deserialize)]
pub enum BenchmarkType {
    Text,
    Image,
    Audio,
    Video,
    Graphics3D,
    Code,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum BenchmarkInput {
    Text { prompt: String },
    Vision { image_path: String, prompt: String },
    Audio { audio_path: String },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub iterations: u32,
    pub warmup_runs: u32,
    pub temperature: f32,
    // ... existing fields
}
```

### Metrics by Type

**Text Metrics:**
- TPS (tokens per second)
- TTFT (time to first token)
- ITL (inter-token latency)
- Total generation time

**Vision Metrics:**
- TTFT (time to first token)
- TPS (description generation speed)
- Image processing time
- Total time

**Audio Metrics:**
- RTF (real-time factor) = processing_time / audio_duration
- Latency to first word
- Total transcription time
- (Future: WER if ground truth available)

### UI: Benchmark Type Selector

```
+--------------------------------------------------------------+
| Benchmark Type                                               |
+--------------------------------------------------------------+
| [Text] [Image] [Audio] [Video] [3D] [Code]                  |
+--------------------------------------------------------------+
```

**Type-specific UI (future):**

| Type | Additional UI Elements |
|------|----------------------|
| Text | Prompt input (existing) |
| Image | Image upload + prompt input |
| Audio | Audio file upload |
| Video | Video file upload + prompt |
| 3D | Model file upload |
| Code | Code prompt input + language selector |

**Initial implementation:** Only Text is active, others show "Coming Soon" badge.

### Backend Changes for Multi-Modal

**New endpoint structure:**
```
POST /api/benchmark/text     - Text benchmarks (existing)
POST /api/benchmark/vision   - Vision benchmarks
POST /api/benchmark/audio    - Audio benchmarks
```

Or unified:
```
POST /api/benchmark
{
  "type": "vision",
  "model_id": "llava:7b",
  "image_url": "...",
  "prompt": "Describe this image"
}
```

### Ollama API Support

- **Text**: `/api/chat` with messages
- **Vision**: `/api/chat` with images array in message
- **Audio**: Requires whisper model or external service

```rust
// Vision request to Ollama
{
  "model": "llava:7b",
  "messages": [{
    "role": "user",
    "content": "What's in this image?",
    "images": ["base64_encoded_image"]
  }]
}
```

## 1. Live Text Output (SSE Streaming)

### Current State
- POST `/api/benchmark` blocks until all iterations complete
- No feedback during run except spinner
- User can't see what's happening

### Proposed Architecture

**New SSE Endpoint:** `GET /api/benchmark/stream`

```
Client                    Server
  |-- POST /benchmark ------>|  (initiate, returns immediately)
  |<---- 202 Accepted -------|
  |                          |
  |-- GET /stream ---------->|  (connect SSE)
  |<---- SSE events ---------|
  |    { type: "warmup", current: 1, total: 2 }
  |    { type: "iteration_start", iteration: 1 }
  |    { type: "token", text: "The" }
  |    { type: "token", text: " concept" }
  |    { type: "iteration_complete", metrics: {...} }
  |    { type: "complete", summary: {...} }
```

### Event Types

```rust
#[derive(Serialize)]
#[serde(tag = "type")]
pub enum BenchmarkEvent {
    #[serde(rename = "warmup")]
    Warmup { current: u32, total: u32 },

    #[serde(rename = "iteration_start")]
    IterationStart { iteration: u32, total: u32, prompt: String },

    #[serde(rename = "token")]
    Token { text: String },

    #[serde(rename = "iteration_complete")]
    IterationComplete { iteration: u32, metrics: BenchmarkMetrics },

    #[serde(rename = "complete")]
    Complete { summary: BenchmarkSummary },

    #[serde(rename = "cancelled")]
    Cancelled,

    #[serde(rename = "error")]
    Error { message: String },
}
```

### Backend Changes

**Files to modify:**
- `llamaburn-server/src/main.rs` - New streaming endpoint
- `llamaburn-benchmark/src/ollama.rs` - Add streaming chat method
- `llamaburn-benchmark/src/runner.rs` - Accept event channel, emit events

**OllamaClient additions:**
```rust
pub async fn chat_stream(
    &self,
    model: &str,
    prompt: &str,
    options: ChatOptions,
) -> Result<impl Stream<Item = Result<StreamChunk>>> {
    // Set stream: true in request
    // Return chunked response stream
}
```

**BenchmarkRunner changes:**
```rust
pub async fn run_streaming(
    &self,
    config: &BenchmarkConfig,
    prompts: &[String],
    cancel_token: CancellationToken,
    event_tx: mpsc::Sender<BenchmarkEvent>,
) -> Result<BenchmarkResult> {
    // Emit events as iterations progress
    // Stream tokens from each chat call
}
```

### Frontend Changes

**Files to modify:**
- `llamaburn-web/src/pages/benchmark.rs` - SSE handling, live display
- `llamaburn-web/src/api.rs` - Streaming API function
- `llamaburn-web/style.css` - Live output styling

**New UI components:**
```rust
// Live output panel (center of screen)
<div class="live-output-panel">
    <div class="output-header">
        <span>"Iteration " {iteration} "/" {total}</span>
        <span class="prompt-preview">{current_prompt}</span>
    </div>
    <pre class="live-text">{streaming_text}</pre>
    <div class="iteration-metrics">
        // Show metrics after each iteration completes
    </div>
</div>
```

---

## 2. Benchmark History

### Storage Strategy
- **localStorage** for MVP (no backend changes)
- Key: `llamaburn_benchmark_history`
- Store last 50 runs max

### Data Structure

```rust
#[derive(Serialize, Deserialize)]
pub struct BenchmarkHistoryEntry {
    pub id: String,              // UUID
    pub timestamp: i64,          // Unix timestamp
    pub benchmark_type: BenchmarkType,  // Text, Vision, or Audio
    pub model_id: String,
    pub config: BenchmarkConfig,
    pub summary: BenchmarkSummary,
    pub metrics: Vec<BenchmarkMetrics>,  // Individual iterations
}

#[derive(Serialize, Deserialize)]
pub struct BenchmarkHistory {
    pub entries: Vec<BenchmarkHistoryEntry>,
}
```

### UI Components

**History panel (below results):**
```
+------------------------------------------+
| History                           [Clear] |
+------------------------------------------+
| Model           | TPS    | TTFT  | Date  |
|-----------------|--------|-------|-------|
| llama3.1:8b     | 45.2   | 89ms  | Today |
| gemma3:27b      | 28.1   | 142ms | Today |
| llama3.1:8b     | 44.8   | 91ms  | Yest. |
+------------------------------------------+
         [Compare Selected]
```

### Implementation

**New file:** `llamaburn-web/src/storage.rs`
```rust
pub fn save_benchmark_result(entry: BenchmarkHistoryEntry) { ... }
pub fn load_benchmark_history() -> BenchmarkHistory { ... }
pub fn clear_benchmark_history() { ... }
```

---

## 3. Model Comparison

### UI Layout

When 2+ history entries selected:
```
+--------------------------------------------------+
| Comparison: llama3.1:8b vs gemma3:27b            |
+--------------------------------------------------+
|              | llama3.1:8b | gemma3:27b | Diff   |
|--------------|-------------|------------|--------|
| Avg TPS      | 45.2        | 28.1       | +60.8% |
| Avg TTFT     | 89ms        | 142ms      | -37.3% |
| Min TPS      | 42.1        | 26.5       | +58.9% |
| Max TPS      | 48.3        | 30.2       | +59.9% |
+--------------------------------------------------+
```

### Implementation

**New component:** `llamaburn-web/src/components/comparison.rs`
```rust
#[component]
pub fn ComparisonTable(entries: Vec<BenchmarkHistoryEntry>) -> impl IntoView {
    // Render comparison grid
    // Highlight winner for each metric
    // Show percentage differences
}
```

---

## Implementation Order

**Starting with Text benchmarking, other modalities to follow.**

### Phase 1: Benchmark Type Foundation
1. Add BenchmarkType enum to llamaburn-core (all 6 types defined)
2. Add type selector UI (tabs, Text selected by default)
3. Update BenchmarkConfig to include type
4. Only Text is functional initially, others show "Coming Soon"

### Phase 2: Live Text Output (SSE Streaming)
1. Add streaming support to OllamaClient (`chat_stream` method)
2. Create BenchmarkEvent enum and streaming runner
3. Add SSE endpoint `/api/benchmark/stream` to server
4. Update frontend with EventSource handling
5. Add live output panel UI (center of screen)

### Phase 3: Benchmark History
1. Create localStorage helpers (`storage.rs`)
2. Save results after benchmark completes (with type)
3. Add history panel below results
4. Implement clear functionality

### Phase 4: Model Comparison
1. Add checkbox selection to history entries
2. Create comparison component
3. Wire up "Compare Selected" button

### Phase 5: Model Metadata

Pull and display model information from available APIs.

**Data Sources:**

| Source | Endpoint | Data Available | Auth Required |
|--------|----------|----------------|---------------|
| Ollama | `POST /api/show` | Parameter size, quantization, family, format | No |
| HuggingFace | `GET /api/models/{id}` | Downloads, license, architecture, tags | No (optional for rate limits) |

**Ollama `/api/show` Response:**
```json
{
  "modelfile": "...",
  "parameters": "...",
  "template": "...",
  "details": {
    "format": "gguf",
    "family": "llama",
    "parameter_size": "7B",
    "quantization_level": "Q4_K_M"
  }
}
```

**HuggingFace API Response (subset):**
```json
{
  "id": "meta-llama/Meta-Llama-3-8B",
  "downloads": 1234567,
  "license": "llama3",
  "tags": ["text-generation", "pytorch"],
  "library_name": "transformers"
}
```

**Data Structure:**
```rust
pub struct ModelInfo {
    pub model_id: String,
    pub parameter_size: Option<String>,  // "7B"
    pub quantization: Option<String>,    // "Q4_K_M"
    pub family: Option<String>,          // "llama"
    pub format: Option<String>,          // "gguf"
    // HuggingFace (optional)
    pub hf_downloads: Option<u64>,
    pub license: Option<String>,
}
```

**UI Display (below model selector):**
```
Model: llama3:7b
├─ Size: 7B params
├─ Quant: Q4_K_M
├─ Family: llama
└─ Format: gguf
```

**Implementation:**
1. `OllamaClient::show_model()` - fetch from Ollama
2. `ModelInfoService` - aggregate data from sources
3. Async fetch on model selection change
4. Cache results to avoid repeated API calls

**HuggingFace Mapping Challenge:**
Ollama model names don't directly map to HF repo names:
- `llama3:7b` → `meta-llama/Meta-Llama-3-8B`
- `gemma3:27b` → `google/gemma-3-27b`

Options:
1. Heuristic parsing (strip version tags, search HF)
2. Maintain a mapping table
3. Skip HF for non-matching models

### Future Phases (After Text is Complete)
- Phase 6: Image benchmarking
- Phase 7: Audio benchmarking
- Phase 8: Video benchmarking
- Phase 9: Code benchmarking
- Phase 10: 3D Graphics benchmarking

---

## Files to Create/Modify

### New Files
- `llamaburn-core/src/benchmark_type.rs` - BenchmarkType enum
- `llamaburn-web/src/storage.rs` - localStorage helpers
- `llamaburn-web/src/components/comparison.rs` - Comparison table
- `llamaburn-web/src/components/type_selector.rs` - Benchmark type tabs

### Modified Files
- `llamaburn-core/src/lib.rs` - Export BenchmarkType
- `llamaburn-core/src/config.rs` - Add benchmark_type to BenchmarkConfig
- `llamaburn-server/src/main.rs` - SSE streaming endpoint
- `llamaburn-benchmark/src/ollama.rs` - Streaming chat method
- `llamaburn-benchmark/src/runner.rs` - Event-emitting runner
- `llamaburn-web/src/pages/benchmark.rs` - SSE handling, live panel, type selector, history
- `llamaburn-web/src/api.rs` - Streaming API
- `llamaburn-web/style.css` - New component styles

---

## Verification

### Phase 1: Type Selector
1. Verify 6 tabs appear (Text, Image, Audio, Video, 3D, Code)
2. Text is selected by default
3. Other tabs show "Coming Soon" when clicked

### Phase 2: Live Text Output
1. Start benchmark, observe live text streaming in center panel
2. See iteration progress (e.g., "Iteration 2/5")
3. Cancel mid-run, verify stream closes cleanly
4. Complete run, verify final results display

### Phase 3: Benchmark History
1. Run benchmark, verify entry appears in history panel
2. Refresh page, verify history persists (localStorage)
3. Clear history, verify it's gone

### Phase 4: Model Comparison
1. Run benchmarks on 2 different models
2. Select both in history
3. Click Compare, verify comparison table shows
4. Verify diff percentages are calculated correctly
