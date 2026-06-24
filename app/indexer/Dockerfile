# Build with the app/ workspace as context:
#   docker build -f indexer/Dockerfile -t flipvault-indexer .
FROM rust:1.92-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p flipvault-indexer

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/flipvault-indexer /usr/local/bin/flipvault-indexer
ENV RUST_LOG=info
EXPOSE 8080
# Provide DATABASE_URL, RPC_URL, BIND via env.
ENTRYPOINT ["/usr/local/bin/flipvault-indexer"]
