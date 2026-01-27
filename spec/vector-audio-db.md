# Vector Database Analysis for Audio Signal Chain

## Current Architecture Summary

**Audio Signal Chain:**
```
Device Input → Format Conversion → Resampling (16kHz) → Effect Chain → Output
```

**Effect Detection Pipeline:**
- Fx-Encoder++ produces **32-dim embeddings** via contrastive learning
- Signal analysis extracts DSP metrics (cross-correlation, spectral diff, crest factor)
- Results stored in SQLite with JSON serialization

**Storage (SQLite):**
- `effect_detection_history` - stores `effects_json`, no embedding persistence
- `benchmark_history` - stores metrics/summaries as JSON blobs
- Indexing: timestamp, tool type, model_id only

---

## Vector Database Evaluation

### Current Need: **Low**

The 32-dim embeddings from Fx-Encoder++ are:
- Generated on-demand, not persisted
- Used for single-session analysis (dry/wet comparison)
- Not queried for similarity search

Current SQLite approach is adequate for:
- Historical logging
- Benchmark result retrieval
- Basic filtering by tool/timestamp

### Future Extensibility: **Medium-High**

Vector DB becomes valuable when you implement:

| Planned Feature | Vector DB Benefit |
|-----------------|-------------------|
| MusicSeparation | Store stem embeddings for content-based retrieval |
| MusicTranscription | MIDI similarity search |
| MusicGeneration | Prompt-to-audio embedding matching |
| LlmMusicAnalysis | Semantic search over music metadata |

---

## Recommendation

### Short-term (No vector DB needed)
- Persist embeddings in SQLite as BLOB columns
- Add optional embedding similarity via brute-force cosine (32-dim is fast)

### Medium-term (When to add vector DB)
Add when ANY of these occur:
1. Embedding count exceeds ~100K
2. You need sub-100ms similarity search across stored audio
3. MusicGeneration or LlmMusicAnalysis ships

### Suggested Options (when ready)
| Option | Pros | Cons |
|--------|------|------|
| **lancedb** (Rust) | Embedded, no server, Arrow-native | Newer ecosystem |
| **qdrant** (Rust) | Battle-tested, rich filtering | Requires server or embedded mode |
| **sqlite-vss** | SQLite extension, minimal change | Limited features |

---

## Minimal Prep for Future Vector DB

To ease future migration without adding complexity now:

1. **Persist embeddings** - Add `embedding BLOB` column to `effect_detection_history`
2. **Standardize embedding format** - Always use f32 arrays, document dimensionality
3. **Abstract similarity search** - Trait for `find_similar(embedding, top_k)` that starts with brute-force

---

## Verdict

**No vector database needed now.** The 32-dim embeddings are small enough for brute-force search, and current use cases don't require similarity queries over historical data.

**Prep step (optional):** Persist embeddings to SQLite BLOB column for when you do need similarity search later.
