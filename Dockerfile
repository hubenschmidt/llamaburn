FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY agent/Cargo.toml agent/Cargo.lock* ./agent/
COPY agent/crates ./agent/crates

WORKDIR /app/agent
RUN cargo build --release -p llamaburn-cli

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/agent/target/release/llamaburn /usr/local/bin/

CMD ["llamaburn"]
