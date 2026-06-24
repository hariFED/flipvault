# FlipVault

A Solana program (Anchor / Rust) implementing a shared constant-product bonding curve
backing 4 two-tranche vaults, with Switchboard On-Demand VRF driving per-round vault flips.
The game token is **purely virtual** — only SOL ever moves as real lamports.

> Design is being finalized in `docs/` before implementation. See the understanding doc.

## Development environment (Docker)

The full Solana/Anchor/Rust toolchain runs inside a pinned Docker container, so we never
fight native-Windows SBF builds. Your **source stays on the host**; builds, tests, and the
local validator run in the container. Heavy state (`target/`, Cargo registry, Solana keypair)
lives in named Docker volumes for speed.

### Prerequisites
- Docker Desktop (WSL2 backend) — already present on this machine.

### First-time setup
```powershell
./dev.ps1 build      # build the toolchain image (slow the first time — Anchor compiles from source)
./dev.ps1 up         # start the dev container (stays running in the background)
./dev.ps1 versions   # sanity-check rust / solana / anchor / node
```

### Everyday commands
```powershell
./dev.ps1 shell           # bash shell inside the container
./dev.ps1 build-program   # anchor build
./dev.ps1 test            # anchor test
./dev.ps1 validator       # local solana-test-validator (RPC :8899, ws :8900)
./dev.ps1 anchor <args>   # any anchor subcommand
./dev.ps1 cargo  <args>   # any cargo subcommand
./dev.ps1 down            # stop the container
./dev.ps1 clean           # stop + delete volumes (full reset)
```

You can also drop the wrapper and use `docker compose ...` directly.

### Pinned versions
Set in `Dockerfile` (overridable via `docker-compose.yml` build args):

| Tool | Version |
|------|---------|
| Rust | 1.92.0 |
| Agave (Solana CLI) | stable |
| Anchor | 0.31.1 |
| Node | 22 |

### Notes
- The Anchor workspace lives in `flipvault/`; the container's working dir is `/workspace/flipvault`.
- The first `anchor build` downloads Solana's SBF **platform-tools** (a few hundred MB); dependency compiles are cached in the `cargo-registry`/`cargo-git` volumes.
- Build artifacts (`flipvault/target/`, IDL, `.so`, program keypair) live on the Windows host — visible directly and persistent across container resets.
- Switchboard On-Demand VRF testing typically runs against devnet oracles; local-validator VRF wiring will be documented when the VRF flow lands.
