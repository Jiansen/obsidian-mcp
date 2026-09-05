FROM rust:1-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN cargo build --release

# Use trixie (same as rust:1-slim base) to match glibc
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3t64 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/obsidian-mcp /usr/local/bin/obsidian-mcp

WORKDIR /vault
EXPOSE 37842

ENTRYPOINT ["obsidian-mcp"]
CMD ["--http", "--port", "37842", "--host", "0.0.0.0", "/vault"]
