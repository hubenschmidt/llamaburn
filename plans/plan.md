# Agent Codebase Simplification Plan

## Summary
Refactor the Rust agent codebase to eliminate the 5 major DRY violations.

**Scope:** All items (High, Medium, Low priority)

---

## High Priority (DRY Violations)

### 1. Extract LLM error mapping helper
**File:** `agent/crates/agents-llm/src/client.rs`

`.map_err(|e| AgentError::LlmError(e.to_string()))` appears 13 times.

**Fix:** Add helper trait or function:
```rust
fn to_llm_error<E: ToString>(e: E) -> AgentError {
    AgentError::LlmError(e.to_string())
}
```

### 2. Extract response content extraction
**File:** `agent/crates/agents-llm/src/client.rs`

Same 5-line pattern appears 3 times (lines 68-72, 131-135, 182-186).

**Fix:** Add helper method:
```rust
fn extract_content(response: &CreateChatCompletionResponse) -> Result<String, AgentError>
```

### 3. Remove redundant constructors
**File:** `agent/crates/agents-llm/src/client.rs`

- Remove `with_model()` (lines 27-29) - just use `new()`
- Remove `chat()` (lines 31-34) - just use `chat_with_model()` directly

### 4. Extract worker result helper
**Files:** `agent/crates/agents-workers/src/{general,search,email}.rs`

Same 15-line match pattern for building `WorkerResult`.

**Fix:** Add helper in `agents-core`:
```rust
impl WorkerResult {
    pub fn ok(output: String) -> Self { ... }
    pub fn err(e: impl ToString) -> Self { ... }
}
```

### 5. Extract feedback formatting
**Files:** `agent/crates/agents-workers/src/{general,search,email}.rs`

Same 3-line pattern appears 3 times.

**Fix:** Add helper function in `prompts.rs` or inline.

---

## Medium Priority (YAGNI)

### 6. Remove `WorkerType::None`
**File:** `agent/crates/agents-core/src/types.rs:9`

Unused enum variant. Delete it.

### 7. Simplify EmailWorker LLM logic
**File:** `agent/crates/agents-workers/src/email.rs`

Currently always calls LLM even when body is provided. Skip LLM call if body param is non-empty.

### 8. Validate API keys at construction
**Files:** `agent/crates/agents-workers/src/{search,email}.rs`

Move empty API key checks from `execute()` to `new()`. Fail fast.

### 9. Remove `SearchResult` intermediate struct
**File:** `agent/crates/agents-workers/src/search.rs:85-89`

Inline the formatting directly from `SerpApiResponse`.

---

## Low Priority (Code Quality)

### 10. Use enum for Message role
**File:** `agent/crates/agents-core/src/types.rs`

Change `role: String` to `role: MessageRole` enum.

### 11. Return reference from conversation get
**File:** `agent/crates/agents-server/src/state.rs:49`

Avoid cloning entire history. Return `&Vec<Message>` or use `Arc`.

### 12. Simplify WebSocket uuid handling
**File:** `agent/crates/agents-server/src/ws.rs`

Make uuid required on init, store once, don't re-check every message.

---

## Files to Modify

| File | Changes |
|------|---------|
| `agent/crates/agents-core/src/types.rs` | Add `WorkerResult` helpers, remove `WorkerType::None`, add `MessageRole` enum |
| `agent/crates/agents-llm/src/client.rs` | Extract error helper, extract content helper, remove redundant methods |
| `agent/crates/agents-workers/src/general.rs` | Use new `WorkerResult` helpers |
| `agent/crates/agents-workers/src/search.rs` | Use helpers, remove `SearchResult`, validate key in `new()` |
| `agent/crates/agents-workers/src/email.rs` | Use helpers, skip LLM when body provided, validate key in `new()` |
| `agent/crates/agents-server/src/state.rs` | Return reference not clone |
| `agent/crates/agents-server/src/ws.rs` | Simplify uuid handling |

---

## Estimated Impact
- ~100 lines removed
- 7 files modified
- No new dependencies
