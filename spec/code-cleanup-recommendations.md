# Llamaburn Code Cleanup Recommendations

## Overview

This document identifies technical debt, performance anti-patterns, and structural issues across the Llamaburn codebase, with prioritized recommendations for cleanup.

---

## 1. Structural Issues (High Priority)

### 1.1 Circular Coupling

**Problem:** `llamaburn-services` and `llamaburn-gui/panels/benchmark` have bidirectional dependencies, creating unclear layering and making isolated testing difficult.

**Recommendation:** Extract shared types to `llamaburn-core` or introduce a dedicated interface crate. Services should not depend on GUI concerns.

### 1.2 Monolithic Files

Files exceeding 500 lines that should be decomposed:

| File | Lines | Recommended Split |
|------|-------|-------------------|
| `llamaburn-gui/src/panels/benchmark/mod.rs` | 939 | Extract state management, UI rendering, event handling into submodules |
| `llamaburn-gui/src/panels/benchmark/audio/stt.rs` | 684 | Separate recording logic from transcription display |
| `llamaburn-services/src/whisper.rs` | 663 | Split into `config.rs`, `streaming.rs`, `batch.rs` |
| `llamaburn-services/src/audio_input.rs` | 618 | Extract device enumeration, stream management, buffer handling |

### 1.3 Wildcard Re-exports

**Location:** `llamaburn-core/src/lib.rs`

**Problem:** `pub use` wildcards expose internal implementation details and create implicit API boundaries.

**Recommendation:** Replace with explicit re-exports:
```rust
// Before
pub use crate::types::*;

// After
pub use crate::types::{BenchmarkResult, ModelConfig, AudioFormat};
```

---

## 2. Performance Anti-Patterns

### 2.1 Unnecessary `.clone()` Calls (~60 instances)

**Severity:** High

**Hotspots in `ollama.rs`:**
- Line 124: `self.host.clone()`
- Line 139: `self.host.clone()`
- Line 179: `self.host.clone()`
- Line 215: `self.host.clone()`
- Line 274: `self.host.clone()`

**Pattern:** Host string cloned on every API call.

**Fix:** Store `Arc<str>` or pass `&str` to request builders:
```rust
// Before
let url = format!("{}/api/generate", self.host.clone());

// After
let url = format!("{}/api/generate", &self.host);
```

### 2.2 `format!` in Hot Paths (~10 instances)

**Severity:** Medium

**Location:** `code_executor.rs:86-150`

**Problem:** Format strings allocated per test case iteration.

**Fix:** Pre-allocate or use `write!` to a reusable buffer:
```rust
// Before (in loop)
let output = format!("Test {}: {}", i, result);

// After
let mut buf = String::with_capacity(256);
for (i, result) in results.iter().enumerate() {
    buf.clear();
    write!(&mut buf, "Test {}: {}", i, result).unwrap();
    // use buf
}
```

### 2.3 Sequential Awaits (4 areas)

**Severity:** Medium

**Problem:** Independent async operations awaited sequentially instead of concurrently.

**Fix:** Use `tokio::join!` or `futures::join!`:
```rust
// Before
let a = fetch_a().await;
let b = fetch_b().await;

// After
let (a, b) = tokio::join!(fetch_a(), fetch_b());
```

### 2.4 Pre-collection Before Reduction

**Location:** `audio_input.rs:287-310`

**Problem:** Vec allocation before `min_by_key` operation.

**Fix:** Use iterator directly:
```rust
// Before
let devices: Vec<_> = iter.collect();
let best = devices.iter().min_by_key(|d| d.latency);

// After
let best = iter.min_by_key(|d| d.latency);
```

---

## 3. Code Duplication

### 3.1 Segment Extraction (whisper.rs)

**Occurrences:** 3 locations
**Lines saved:** ~50

**Pattern:**
```rust
// Repeated logic for extracting segments from whisper output
let segments: Vec<Segment> = state.full_n_segments()
    .map(|i| {
        let start = state.full_get_segment_t0(i);
        let end = state.full_get_segment_t1(i);
        let text = state.full_get_segment_text(i);
        Segment { start, end, text }
    })
    .collect();
```

**Fix:** Extract to helper function `fn extract_segments(state: &WhisperState) -> Vec<Segment>`.

### 3.2 Whisper Config Construction

**Occurrences:** 3 locations
**Lines saved:** ~15

**Fix:** Create `WhisperConfig::default_streaming()` and `WhisperConfig::default_batch()` constructors.

### 3.3 Language Execution Wrappers (code_executor.rs)

**Occurrences:** Multiple language handlers
**Lines saved:** ~40

**Problem:** Each language (Python, Go, Rust, JavaScript) has nearly identical execution wrapper logic.

**Fix:** Implement trait-based dispatch:
```rust
trait LanguageExecutor {
    fn compile(&self, source: &str) -> Result<PathBuf>;
    fn execute(&self, binary: &Path, input: &str) -> Result<String>;
}
```

### 3.4 Error Handling Boilerplate (whisper.rs)

**Occurrences:** 9+ locations
**Lines saved:** ~20

**Pattern:** Repetitive `match` on whisper results with identical error mapping.

**Fix:** Use `?` operator with custom `From` impl or a helper macro.

---

## 4. Oversized Functions

Functions exceeding 50 lines that should be decomposed:

| Function | File | Lines | Recommendation |
|----------|------|-------|----------------|
| `run_problem()` | `code_runner.rs` | 105 | Split into `prepare_environment()`, `execute_tests()`, `collect_results()` |
| `transcribe_samples_streaming()` | `whisper.rs` | 77 | Extract buffer management into separate function |

---

## 5. Priority Matrix

| Priority | Category | Estimated Impact | Effort |
|----------|----------|------------------|--------|
| P0 | Remove unnecessary clones in ollama.rs | High (API latency) | Low |
| P1 | Decompose benchmark/mod.rs | High (maintainability) | Medium |
| P1 | Extract segment extraction helper | Medium (DRY) | Low |
| P2 | Fix sequential awaits | Medium (throughput) | Low |
| P2 | Explicit re-exports in core | Medium (API clarity) | Low |
| P3 | Language executor trait | Low (extensibility) | Medium |
| P3 | Format! optimization | Low (microbench only) | Low |

---

## 6. Recommended Refactoring Order

1. **Quick wins (P0):** Remove `.clone()` calls in `ollama.rs` - immediate perf gain
2. **DRY helpers:** Extract `extract_segments()` in `whisper.rs`
3. **Module split:** Break `benchmark/mod.rs` into logical submodules
4. **Async optimization:** Convert sequential awaits to concurrent
5. **API boundaries:** Replace wildcard exports in `llamaburn-core`

---

## Appendix: File Size Summary

```
llamaburn-gui/src/panels/benchmark/mod.rs        939 lines
llamaburn-gui/src/panels/benchmark/audio/stt.rs  684 lines
llamaburn-services/src/whisper.rs                663 lines
llamaburn-services/src/audio_input.rs            618 lines
llamaburn-services/src/ollama.rs                 ~400 lines
llamaburn-services/src/code_executor.rs          ~350 lines
```

---

*Generated: 2026-01-18*
