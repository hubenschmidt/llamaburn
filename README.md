# svelte-rust-agents-sdk

A multi-agent chat system with a Svelte 5 frontend and Rust backend featuring real-time streaming, LLM observability, and modular worker architecture.

## Features

- **Real-time streaming** — Responses stream token-by-token as they're generated
- **Multi-agent pipeline** — Frontline → Orchestrator → Workers → Evaluator
- **LLM observability** — Token usage and response time displayed per message
- **Modular workers** — Search (Serper), Email (SendGrid), General conversation
- **Evaluator loop** — Optional quality validation with retry logic

## Architecture

```
┌─────────────────┐     WebSocket      ┌─────────────────────────────────────┐
│                 │◄──────────────────►│              Agent                  │
│  Svelte 5 UI    │    (streaming)     │                                     │
│  (SvelteKit)    │                    │  ┌─────────┐    ┌─────────────┐     │
│                 │                    │  │Frontline│───►│ Orchestrator│     │
└─────────────────┘                    │  └─────────┘    └──────┬──────┘     │
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

**Agent Crates:**
- `agents-core` — Shared types, errors, and traits
- `agents-llm` — OpenAI client with streaming support
- `agents-pipeline` — Frontline, Orchestrator, Evaluator agents
- `agents-workers` — Search, Email, General worker implementations
- `agents-server` — Axum WebSocket server

## Technologies Used

- **Client:** Svelte 5, SvelteKit, TypeScript
- **Agent:** Rust 1.92, Axum, Tokio
- **LLM:** OpenAI API

## Prerequisites

- Docker & Docker Compose

## Environment Variables

Create a `.env` file in the project root:

```env
# Required
OPENAI_API_KEY=sk-...

# Optional
OPENAI_MODEL=gpt-4o           # Main model for routing decisions
WORKER_MODEL=gpt-4o-mini      # Model for workers
SERPER_API_KEY=...            # For web search (serper.dev)
SENDGRID_API_KEY=...          # For email sending
SENDGRID_FROM_EMAIL=noreply@example.com
RUST_LOG=info
```

## Run

```bash
docker compose up
```

- Client: http://localhost:3000
- Agent: http://localhost:8000

