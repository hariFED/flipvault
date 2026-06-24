# FlipVault dev toolchain — Rust + Agave (Solana) + Anchor + Node for SBF program dev.
# Pinned, reproducible Linux toolchain so we never fight native-Windows Solana builds.
FROM ubuntu:22.04

# Versions are ARGs so they can be bumped from docker-compose without editing this file.
ARG RUST_VERSION=1.92.0
ARG SOLANA_VERSION=stable          # "stable" or a pinned tag like "v2.1.21"
ARG ANCHOR_VERSION=0.31.1
ARG NODE_MAJOR=22

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/root/.cargo
ENV RUSTUP_HOME=/root/.rustup
ENV PATH=/root/.cargo/bin:/root/.local/share/solana/install/active_release/bin:/usr/local/bin:$PATH

# System deps for building Solana programs and Rust crates with native bindings.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config libssl-dev libudev-dev zlib1g-dev \
      llvm clang cmake libclang-dev protobuf-compiler \
      curl ca-certificates git bzip2 xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Node.js (Anchor's TS test harness) + yarn via corepack.
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && corepack enable \
    && rm -rf /var/lib/apt/lists/*

# Rust (host-side). SBF program builds use Solana's bundled platform-tools, not this toolchain.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain ${RUST_VERSION} --profile minimal -c rustfmt -c clippy

# Agave / Solana CLI.
RUN sh -c "$(curl -sSfL https://release.anza.xyz/${SOLANA_VERSION}/install)"

# Use the system git client for cargo fetches (more reliable for large repos than the
# built-in libgit2 path, which is prone to timeouts in-container).
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
# Anchor CLI, pinned, built from source. avm's prebuilt-binary download timed out in-container;
# building from source uses the same git+crates path that already works.
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v${ANCHOR_VERSION} anchor-cli --force \
    && anchor --version

WORKDIR /workspace
CMD ["sleep", "infinity"]
