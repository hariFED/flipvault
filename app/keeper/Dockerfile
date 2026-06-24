# Build with the app/ workspace as context:
#   docker build -f keeper/Dockerfile -t flipvault-keeper .
FROM rust:1.92-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p flipvault-keeper

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/flipvault-keeper /usr/local/bin/flipvault-keeper
ENV RUST_LOG=info
# Provide RPC_URL, ORAO_TREASURY, POLL_SECS via env; mount the keypair and set KEEPER_KEYPAIR.
ENTRYPOINT ["/usr/local/bin/flipvault-keeper"]
