# LlamaBurn Development Guide

## Prerequisites

See [DEPENDENCIES.md](DEPENDENCIES.md) for complete build/runtime dependency list.

**Quick start (Ubuntu/Debian):**
```bash
sudo apt install cmake clang libasound2-dev \
    libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libgl1-mesa-dev libwayland-dev
```

### Build

```bash
cd agent
cargo build --release -p llamaburn-gui
```

Includes ROCm GPU-accelerated Whisper (hipBLAS) and live microphone recording by default.

## Development (Hot Reload)

```bash
cd agent

# Install cargo-watch (once)
cargo install cargo-watch

# Run with hot reload
cargo watch -x 'run -p llamaburn-gui'
```

Changes to any `.rs` file trigger automatic rebuild (~1-2s incremental).

## Logging

Logs are written to both stdout and `/tmp/llamaburn.log`.

**Watch logs in separate terminal:**
```bash
tail -f /tmp/llamaburn.log
```

**Example output:**
```
INFO llamaburn_gui: LlamaBurn GUI starting
INFO llamaburn_services::ollama: Starting async model fetch
INFO llamaburn_services::gpu_monitor: Starting GPU monitor subscription
INFO llamaburn_services::ollama: Fetched models from Ollama count=3
INFO llamaburn_services::gpu_monitor: GPU monitor connected
```

**Adjust log levels:**
```bash
# Default: services=debug, gui=info
RUST_LOG=llamaburn_services=trace cargo watch -x 'run -p llamaburn-gui'
```

Levels: `error` < `warn` < `info` < `debug` < `trace`

## Release Build

```bash
cd agent
cargo build --release -p llamaburn-gui
```

Binary: `target/release/llamaburn-gui`

## Project Structure

```
agent/
├── crates/
│   ├── llamaburn-gui/      # Desktop GUI (egui)
│   ├── llamaburn-core/     # Shared types
│   ├── llamaburn-benchmark/# Benchmark logic
│   └── ...
├── Cargo.toml              # Workspace config
├── Cargo.lock              # Locked dependencies
└── rust-toolchain.toml     # Pinned Rust version
```

## Database & Migrations

LlamaBurn uses SQLite for benchmark history and settings, with **refinery** for schema migrations.

**Database location:**
```
~/.local/share/llamaburn/history.db
```

**Migrations location:**
```
agent/crates/llamaburn-services/migrations/
├── V1__initial_schema.sql    # benchmark_history table
├── V2__settings_table.sql    # settings key-value store
```

### How Migrations Work

Migrations are **embedded at compile time** via `refinery::embed_migrations!` and run automatically on app startup. Refinery creates a `refinery_schema_history` table to track which migrations have been applied.

### Connect to Database

```bash
# Install sqlite3 if needed
sudo apt install sqlite3

# Connect interactively
sqlite3 ~/.local/share/llamaburn/history.db

# Useful commands inside sqlite3:
sqlite> .tables              # List all tables
sqlite> .schema              # Show all table schemas
sqlite> .headers on          # Show column headers in output
sqlite> .mode column         # Pretty print output
sqlite> SELECT * FROM benchmark_history;
sqlite> .quit                # Exit
```

### Inspect Database (One-liners)

```bash
# List tables
sqlite3 ~/.local/share/llamaburn/history.db ".tables"

# View migration history
sqlite3 ~/.local/share/llamaburn/history.db "SELECT * FROM refinery_schema_history"

# View schema
sqlite3 ~/.local/share/llamaburn/history.db ".schema"

# Query benchmark history
sqlite3 ~/.local/share/llamaburn/history.db "SELECT model_id, json_extract(summary_json, '$.avg_tps') as tps FROM benchmark_history"
```

### Reset Database (Fresh Start)

**Option 1: Delete the database file**
```bash
rm ~/.local/share/llamaburn/history.db
# Migrations re-run automatically on next app launch
```

**Option 2: Programmatic reset** (available via `HistoryService::reset_database()`)
- Drops all tables including `refinery_schema_history`
- Re-runs all migrations from scratch

### Adding New Migrations

1. Create a new SQL file in `migrations/` with format `V{N}__{description}.sql`
   ```
   V3__add_model_metadata.sql
   ```

2. Write your SQL:
   ```sql
   CREATE TABLE model_metadata (
       model_id TEXT PRIMARY KEY,
       info_json TEXT NOT NULL,
       fetched_at INTEGER NOT NULL
   );
   ```

3. Rebuild and run - migrations apply automatically

**Important:** Refinery does not support "down" migrations. To rollback:
- For dev: delete the database and re-run
- For prod: write a new migration that undoes changes

### Troubleshooting

**"table already exists" error:**
- Database has old schema from before refinery
- Solution: Delete `~/.local/share/llamaburn/history.db`

**Migrations not running:**
- Check `refinery_schema_history` table for applied versions
- Ensure migration files follow `V{N}__name.sql` naming

## GUI Framework

- **egui** - Immediate mode GUI library
- **eframe** - Native window/rendering backend

Future additions:
- `egui_plot` for charts
- `wgpu` for 3D rendering
- AppImage packaging for distribution
