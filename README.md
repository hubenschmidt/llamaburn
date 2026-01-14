# svelte-rust-agents-sdk

A multi-agent chat system with a Svelte 5 frontend and Rust backend featuring real-time streaming, LLM observability, and modular worker architecture.

- **Real-time streaming** — Responses stream token-by-token as they're generated
- **Multi-agent pipeline** — Frontline, Orchestrator, Workers, Evaluator
- **Local model support** — Auto-discovers Ollama models at startup
- **Benchmarking mode** — Toggle verbose metrics (tokens/sec, eval time, load time)
- **LLM observability** — Token usage and response time displayed per message
- **Modular workers** — Search (Serper), Email (SendGrid), General conversation

## Architecture

```
┌─────────────────┐                    ┌─────────────────────────────────────┐
│                 │◄──── WebSocket ────│              Agent                  │
│  Svelte 5 UI    │                    │                                     │
│                 │                    │  Model Discovery                    │
│  Settings       │                    │  ├── OpenAI (cloud)                 │
│  └─ Dev Mode    │                    │  └── Ollama /api/tags (local)       │
└─────────────────┘                    │                                     │
                                       │  ┌─────────┐    ┌─────────────┐     │
                                       │  │Frontline│───►│ Orchestrator│     │
                                       │  └─────────┘    └──────┬──────┘     │
                                       │                        │            │
                                       │         ┌──────────────┼──────────┐ │
                                       │         ▼              ▼          ▼ │
                                       │    ┌────────┐    ┌────────┐  ┌─────┐│
                                       │    │ Search │    │ Email  │  │ Gen ││
                                       │    │(Serper)│    │(SGGrid)│  │     ││
                                       │    └────────┘    └────────┘  └─────┘│
                                       │                        │            │
                                       │                   ┌────▼────┐       │
                                       │                   │Evaluator│       │
                                       │                   └─────────┘       │
                                       └─────────────────────────────────────┘
```

## Technologies Used

- **Client:** Svelte 5, SvelteKit, TypeScript
- **Agent:** Rust 1.92, Axum, Tokio
- **LLM:** OpenAI API, Ollama (local models)

## Prerequisites

- Docker & Docker Compose
- (Optional) [Ollama](https://ollama.ai) for local models

## Environment

Create a `.env` file in `agent/`:

```env
# Required
OPENAI_API_KEY=sk-...

# Optional
SERPER_API_KEY=...            # For web search (serper.dev)
SENDGRID_API_KEY=...          # For email sending
SENDGRID_FROM_EMAIL=noreply@example.com
RUST_LOG=info
```

## Run

```bash
docker compose up
```

- Client: http://localhost:3001
- Agent: http://localhost:8000

## Local Models (Ollama)

The agent auto-discovers installed Ollama models at startup via `/api/tags`.

### Setup

1. Install [Ollama](https://ollama.ai)
2. Pull models: `ollama pull llama3.1`
3. Start Ollama: `ollama serve`
4. Start the agent — models appear in the dropdown

### Supported Features

- All installed models automatically listed
- Quantization variants shown (e.g., `Q4_1`, `Q8_0`)
- Model warm-up on selection (pre-loads for faster first response)

## Developer Mode (Benchmarking)

Toggle via the settings icon to see detailed performance metrics for local models.

### Metrics Displayed

| Metric | Description |
|--------|-------------|
| **tokens/sec** | Generation speed |
| **eval_ms** | Time spent generating tokens |
| **prompt_eval_ms** | Time spent processing input |
| **load_duration_ms** | Model load time |

### How It Works

When enabled, uses Ollama's native `/api/chat` endpoint instead of OpenAI-compatible `/v1/chat/completions` to access rich metrics not available via the standard API.

## Project Structure

```
.
├── agent/                    # Rust backend
│   └── crates/
│       ├── agents-core/      # Shared types
│       ├── agents-llm/       # LLM client + Ollama integration
│       ├── agents-pipeline/  # Frontline, Orchestrator, Evaluator
│       ├── agents-workers/   # Search, Email, General workers
│       └── agents-server/    # Axum server, WebSocket handler
├── client/                   # SvelteKit frontend
│   └── src/
│       ├── lib/components/   # Settings, ChatMessage, ChatInput
│       ├── lib/stores/       # chat.ts, settings.ts
│       └── routes/           # +page.svelte
└── docker-compose.yml
```
