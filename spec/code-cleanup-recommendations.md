# Llamaburn Code Cleanup Recommendations

## Overview

Analysis of technical debt, performance anti-patterns, and structural issues across the Llamaburn codebase.

---

## 1. Structural Issues

### 1.1 Monolithic Files

Files exceeding 500 lines requiring decomposition:

| File | Lines | Recommended Split |
|------|-------|-------------------|
| `llamaburn-gui/src/panels/benchmark/mod.rs` | 974 | Extract state management, UI rendering, event handling |
| `llamaburn-gui/src/panels/benchmark/audio/stt.rs` | 684 | Separate recording logic from transcription display |
| `llamaburn-services/src/whisper.rs` | 663 | Split into `config.rs`, `streaming.rs`, `batch.rs` |
| `llamaburn-services/src/audio_input.rs` | 618 | Extract device enumeration, stream management |

### 1.2 Wildcard Re-exports

**Location:** `llamaburn-core/src/lib.rs:9-15`

**Problem:** 7 `pub use *` statements expose internal implementation details.

```rust
// Lines 9-15 - replace with explicit exports
pub use audio::*;
pub use benchmark_type::*;
pub use code_benchmark::*;
pub use config::*;
pub use error::*;
pub use metrics::*;
pub use model::*;
```

**Note:** `llamaburn-services/src/lib.rs` already uses explicit re-exports (good pattern).

### 1.3 Circular Dependencies

**Status:** NOT FOUND - dependency flow is properly acyclic:
- core → benchmark → services → gui

---

## 2. Performance Anti-Patterns

### 2.1 Unnecessary `.clone()` Calls (20+ instances)

**Severity:** Medium

**Hotspots in `ollama.rs`:**
| Line | Pattern |
|------|---------|
| 124 | `self.host.clone()` in `fetch_models_async()` |
| 139 | `self.host.clone()` in `create_model_fetcher()` |
| 179 | `self.host.clone()` in `show_model_async()` |
| 215 | `self.host.clone()` in `unload_model_async()` |
| 274 | `self.host.clone()` in `preload_model_async()` |

**Fix:** Pass `&str` to request builders or use `Arc<str>`.

**In `audio_input.rs`:**
- Line 318: `best.clone().with_sample_rate()` - unnecessary intermediate clone
- Line 367, 426: `samples_collected.lock().unwrap().clone()` - full Vec clone from mutex

### 2.2 `format!` in Hot Paths (8 instances)

**Severity:** High

**Location:** `code_executor.rs`
| Lines | Function | Issue |
|-------|----------|-------|
| 86-91 | `run_python()` | Template with 3 interpolations |
| 111-116 | `run_javascript()` | Same pattern |
| 138-150 | `run_rust()` | Multi-line format with embedded code |
| 197-218 | `run_go()` | Multi-line format with JSON parsing |

**Fix:** Pre-allocate templates or use `write!` to reusable buffer.

### 2.3 Sequential Awaits (4 loops)

**Severity:** Medium

**Location:** `runner.rs`
- Lines 52-56: Sequential warmup loop
- Lines 60-66: Sequential iteration loop

**Fix:** Use `tokio::join!` or `futures::future::join_all()` for independent operations.

### 2.4 Vec Allocation Before Reduction

**Status:** Properly optimized - no issues found.

---

## 3. Code Duplication

### 3.1 Segment Extraction (whisper.rs)

**Locations:** Lines 169-190, 231-251, 311-331
**Total duplicated:** 60 lines (20 × 3)

```rust
// Identical pattern repeated 3 times
let num_segments = state.full_n_segments();
let mut segments = Vec::new();
for i in 0..num_segments {
    let Some(seg) = state.get_segment(i) else { continue; };
    let text = seg.to_str()...;
    segments.push(Segment { start_ms, end_ms, text });
}
```

**Fix:** Extract to `fn extract_segments(state: &WhisperState) -> Result<Vec<Segment>>`.

### 3.2 FullParams Configuration (whisper.rs)

**Locations:** Lines 151-155, 213-217 (exact), 278-283 (variant)
**Duplicated:** 10 lines exact + 6 variant

**Fix:** Create `fn default_params(streaming: bool) -> FullParams`.

### 3.3 Language Execution Wrappers (code_executor.rs)

**Functions:** `run_python`, `run_javascript`, `run_rust`, `run_go`
**Duplicated structure:** ~150 lines across 4 functions

**Fix:** Implement trait-based dispatch or template pattern.

---

## 4. Oversized Functions

| Function | File | Lines | Recommendation |
|----------|------|-------|----------------|
| `run_problem()` | code_runner.rs:121-227 | 106 | Split: prompt building, code generation, test execution |
| `transcribe_samples_streaming()` | whisper.rs:263-340 | 78 | Extract callback setup, segment processing |
| `transcribe_with_timing()` | whisper.rs:137-199 | 63 | Extract segment extraction helper |

---

## 5. Priority Matrix

| Priority | Issue | Impact | Effort |
|----------|-------|--------|--------|
| P0 | Extract segment helper (whisper.rs) | High (DRY) | Low |
| P0 | Extract FullParams helper (whisper.rs) | Medium (DRY) | Low |
| P1 | Remove host.clone() in ollama.rs | Medium (perf) | Low |
| P1 | Decompose benchmark/mod.rs | High (maintainability) | Medium |
| P2 | Explicit re-exports in core | Medium (API clarity) | Low |
| P2 | Parallelize warmup loops | Medium (throughput) | Low |
| P3 | Language executor trait | Low (extensibility) | Medium |

---

## 6. Recommended Order

1. **Quick wins:** Extract `extract_segments()` and `default_params()` in whisper.rs
2. **Clone cleanup:** Replace `.clone()` calls in ollama.rs with references
3. **Module split:** Break benchmark/mod.rs into submodules
4. **API boundaries:** Replace wildcard exports in llamaburn-core

---

## 7. Rust Code Rules Compliance

All refactoring should observe `/rust-code-rules`. Current violations:

| Rule | Violation | Location |
|------|-----------|----------|
| Avoid unnecessary `.clone()` | 20+ instances | ollama.rs, audio_input.rs |
| Prefer `&str` over `String` in params | `host: String` cloned repeatedly | ollama.rs:124,139,179,215,274 |
| Guard clauses, avoid nested conditionals | Deeply nested match/if chains | code_runner.rs:194-210 |
| Prefer iterators over manual loops | Manual segment loops | whisper.rs:169-190,231-251,311-331 |
| Use `?` operator | Verbose match-based error handling | whisper.rs (9+ locations) |

**When implementing fixes:**
- Replace `self.host.clone()` with `&self.host` or `Arc<str>`
- Flatten nested conditionals with early returns
- Convert segment loops to iterator chains
- Use `?` with `From` impls for error propagation

---

*Generated: 2026-01-19*
