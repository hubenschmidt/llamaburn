# Plan: Elm/Redux Architecture Refactor

## Goal

Move all **domain state** to `llamaburn-core`, making the GUI a pure view layer that:
1. Receives `&AppState` to render
2. Emits `Action`s for state changes
3. Never mutates state directly

## Progress

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Scaffold Full Structure in Core | Complete |
| 2 | Wire Up GUI Dispatch Loop | Complete |
| 3 | Migrate Text Panel | Complete |
| 4 | Migrate Audio Panel | Pending |
| 5 | Migrate Code Panel | Pending |
| - | Remove Legacy State | Pending |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      llamaburn-gui                          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ViewState (UI-only: expanded panels, scroll, etc.) │   │
│  └─────────────────────────────────────────────────────┘   │
│                         │ render(&AppState)                 │
│                         ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  View Layer (egui rendering, emits Actions)         │   │
│  └─────────────────────────────────────────────────────┘   │
│                         │ Action                            │
└─────────────────────────┼───────────────────────────────────┘
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                     llamaburn-core                          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  AppState (all domain state)                        │   │
│  └─────────────────────────────────────────────────────┘   │
│                         │                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  update(state, action) -> (state, effects)          │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────┼───────────────────────────────────┘
                          │ Effect
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   llamaburn-services                        │
│  (Side effects: DB, network, file I/O)                     │
└─────────────────────────────────────────────────────────────┘
```

## State Split

### AppState (llamaburn-core) - Domain State

```rust
pub struct AppState {
    // Model selection
    pub models: Vec<String>,
    pub selected_model: String,
    pub loading_models: bool,
    pub model_preloading: bool,

    // Active benchmark type
    pub benchmark_type: BenchmarkType,

    // Sub-states
    pub text: TextState,
    pub audio: AudioState,
    pub code: CodeState,

    // Shared output
    pub live_output: String,
    pub progress: String,
    pub error: Option<String>,
}
```

### ViewState (llamaburn-gui) - UI-Only State

```rust
pub struct ViewState {
    // Panel collapse states
    pub config_panel_expanded: bool,
    pub config_panel_height: f32,
    pub live_output_expanded: bool,
    pub effects_rack_expanded: bool,

    // Modal states
    pub show_audio_settings: bool,
    pub show_save_preset_modal: bool,
    pub preset_name_input: String,

    // History selection
    pub delete_confirm: Option<String>,
    pub selected_history_ids: HashSet<String>,
}
```

## Action Enum (Core)

```rust
pub enum Action {
    // Model actions
    RefreshModels,
    ModelsLoaded(Result<Vec<String>, String>),
    SelectModel(String),
    PreloadModel(String),
    ModelPreloaded(Result<(), String>),

    // Benchmark type
    SetBenchmarkType(BenchmarkType),

    // Text benchmark
    TextSetIterations(u32),
    TextStart,
    TextProgress(String),
    TextComplete(BenchmarkSummary),

    // Code benchmark
    CodeToggleModel(String),
    CodeToggleLanguage(Language),
    CodeStartMatrix,
    CodeProblemComplete(CodeBenchmarkMetrics),

    // Output
    AppendOutput(String),
    SetError(Option<String>),
}
```

## Effect System (Core)

```rust
pub enum Effect {
    FetchModels,
    PreloadModel(String),
    RunTextBenchmark(TextBenchmarkConfig),
    RunCodeBenchmark(CodeBenchmarkConfig),
    SaveHistory(HistoryEntry),
    LoadPresets,
}
```

## Update Function (Reducer)

```rust
impl AppState {
    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::RefreshModels => {
                self.loading_models = true;
                vec![Effect::FetchModels]
            }
            Action::ModelsLoaded(Ok(models)) => {
                self.models = models;
                self.loading_models = false;
                vec![]
            }
            Action::TextStart => {
                self.text.running = true;
                self.live_output.clear();
                vec![Effect::RunTextBenchmark(self.text.to_config())]
            }
            // ...
        }
    }
}
```

## Migration Strategy

**Scaffold first, then migrate panels one by one.**

### Phase 1: Scaffold Full Structure in Core (Complete)

**Files created:**
```
llamaburn-core/src/
├── state/
│   ├── mod.rs      # AppState, re-exports
│   ├── text.rs     # TextState
│   ├── audio.rs    # AudioState
│   └── code.rs     # CodeState
├── action.rs       # Action enum (all variants)
├── effect.rs       # Effect enum (all variants)
└── update.rs       # Reducer: update(state, action) -> effects
```

### Phase 2: Wire Up GUI Dispatch Loop (Complete)

**Files created/modified:**
- `llamaburn-gui/src/panels/benchmark/mod.rs` - Hold AppState + ViewState
- `llamaburn-gui/src/panels/benchmark/view_state.rs` - NEW: ViewState
- `llamaburn-gui/src/panels/benchmark/effect_runner.rs` - NEW: execute Effect → spawn async

### Phase 3: Migrate Text Panel (Complete)

- Removed state from `TextBenchmarkPanel` - now pure view
- `render_config` takes `&TextState`, returns `Vec<Action>`
- `render_rankings` takes `&TextState`
- Services moved to `EffectRunner`
- Deleted `text/execution.rs` and `text/history.rs`

### Phase 4: Migrate Audio Panel (Pending)

Same pattern as text panel.

### Phase 5: Migrate Code Panel (Pending)

Same pattern as text panel.

## Main Loop (GUI)

```rust
fn ui(&mut self, ui: &mut egui::Ui) {
    // 1. Poll async results → Actions
    let actions = self.effect_runner.poll();

    // 2. Update state and collect effects
    for action in actions {
        let effects = self.app_state.update(action);
        for effect in effects {
            self.effect_runner.run(effect);
        }
    }

    // 3. Render (pure view) - returns UI-triggered actions
    let ui_actions = self.render(ui, &self.app_state);

    // 4. Process UI actions
    for action in ui_actions {
        let effects = self.app_state.update(action);
        for effect in effects {
            self.effect_runner.run(effect);
        }
    }
}
```

## Verification

1. `cargo build` compiles
2. Text benchmark works end-to-end
3. State flows: UI → Action → Update → Effect → Action → Update
4. No direct mutation in view code

## Decisions Made

- **Scaffold first**: Create full structure in core, then migrate panels one by one
- **Replace Actions**: Create unified `Action` enum in core, replacing panel-specific actions
- **Centralized Services**: All service instances (BenchmarkService, ModelInfoService, OllamaClient) in EffectRunner, making panels truly stateless pure views
