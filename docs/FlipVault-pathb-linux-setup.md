# FlipVault Path-B — Native Linux Setup (live Arcium MPC)

Goal: on a **native Linux machine**, set up the toolchain and run the confidential flip on real
Arcium MPC (`arcium test`). The Path-B code (program, circuits, SDK, validation test) is **already
built** — you're setting up the environment to *run* it.

> Why native Linux: every wall we hit on Windows/WSL was an OS artifact — io_uring blocked by
> Docker's seccomp, RocksDB ledger slow on the Windows bind-mount, and Arcium's localnet unable to
> start MPC node containers from *inside* a container (Docker-out-of-Docker path mismatch). On
> native Linux none of these exist: Docker runs on the host, the project is on the host fs, and
> `arcium localnet` manages the MPC nodes directly.

---

## 0. Machine requirements
- **Ubuntu 22.04 LTS** (recommended — matches the pinned toolchain) or 24.04. x86_64.
- **≥ 16 GB RAM**, **≥ 40 GB free disk**, normal internet.
- A user with `sudo`.

Run everything as your normal user (not root). Commands that need root use `sudo` explicitly.

---

## 1. System packages
```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libssl-dev libudev-dev zlib1g-dev \
  llvm clang cmake libclang-dev protobuf-compiler \
  curl ca-certificates git bzip2 xz-utils
```

## 2. Docker (native) — needed by `arcium localnet` to run the MPC nodes
```bash
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker "$USER"
newgrp docker          # apply group now (or log out/in)
docker run --rm hello-world   # verify
docker compose version        # verify the compose plugin is present
```
> Docker MUST be installed and runnable as your user **before** the Arcium step (§7), because
> `arcup install` pulls the Arx-node container images.

## 3. Rust (1.92, + wasm target for the SDK)
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.92.0 --profile minimal
. "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
rustc --version    # 1.92.0
```

## 4. Solana CLI 3.1.10  (Arcium 0.11.x pins this exact version)
```bash
sh -c "$(curl -sSfL https://release.anza.xyz/v3.1.10/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
solana --version       # solana-cli 3.1.10
solana-keygen new --no-bip39-passphrase   # creates ~/.config/solana/id.json (the dev wallet)
```
Add the PATH line to `~/.bashrc` so it persists:
```bash
echo 'export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"' >> ~/.bashrc
```

## 5. Anchor 1.0.2  (Arcium 0.11.x pins this)
```bash
cargo install --git https://github.com/coral-xyz/anchor --tag v1.0.2 anchor-cli --locked
anchor --version       # anchor-cli 1.0.2
```
(First build compiles from source — a few minutes.)

## 6. Node 22 + Yarn  (for the TS test harness / @arcium-hq/client)
```bash
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo bash -
sudo apt-get install -y nodejs
corepack enable        # provides `yarn`
node --version         # v22.x
```

## 7. Arcium toolchain (arcup → arcium CLI 0.11.2)
```bash
curl --proto '=https' --tlsv1.2 -sSfL https://install.arcium.com/ | bash
# If the one-liner has trouble, do it manually:
#   TARGET=x86_64_linux
#   curl "https://bin.arcium.com/download/arcup_${TARGET}_0.11.1" -o ~/.cargo/bin/arcup && chmod +x ~/.cargo/bin/arcup
arcup install          # installs the arcium CLI + pulls Arx-node images (needs Docker running)
arcium --version       # arcium 0.11.2
```

## 8. Get the Path-B code onto this machine
The code is on branch **`path-b-perpetual`** (committed on the Windows machine). Easiest path:
1. On the **Windows** machine, push it (ask the assistant, or run):
   ```powershell
   cd C:\Users\Abcom\flipsol ; git push origin path-b-perpetual
   ```
2. On **Linux**, clone + checkout:
   ```bash
   git clone <your-repo-url> flipsol && cd flipsol
   git checkout path-b-perpetual
   ```
Alternative (no GitHub): `rsync`/scp the `flipsol` folder over, **excluding** `target/`,
`node_modules/`, `.anchor/`, `path-b/build/`, `path-b/artifacts/` (all regenerated).

## 9. Build the program + circuits
```bash
cd flipsol/path-b
arcium build
```
- First run downloads the SBF `platform-tools` (v1.52, ~0.5 GB) and compiles everything — slow once,
  cached after. Produces `target/deploy/flipvault_pathb.so` + IDL + TS types and `build/*.arcis`.
- **Already-correct in the repo:** the Arcium crates are pinned to **`=0.11.1`** (the CLI is 0.11.2
  but 0.11.2 crates aren't published — 0.11.1 is the matching set). If you ever re-scaffold with
  `arcium init`, re-apply that pin in `programs/*/Cargo.toml` + `encrypted-ixs/Cargo.toml`.

## 10. Verify the math + SDK (no MPC needed — should pass instantly)
```bash
# the curve precision + solvency proofs
cd flipsol/path-b/spikes/curve-precision && cargo test --release && cargo run --release --bin sweep
# expected: 6 tests pass; sweep -> "0 mismatches ... EXACT"

# the SDK (native + wasm)
cd flipsol/app && cargo test -p flipvault-pathb-sdk      # 5 tests pass
```

## 11. ⭐ Run the live confidential flip (the whole point)
```bash
cd flipsol/path-b
arcium test
```
This spins up a local validator + **Cerberus MPC nodes**, deploys the program, uploads the circuits,
and runs `tests/flipvault-pathb.ts`:
`initialize → seed encrypted curve/treasury → register a box with a client-encrypted 5 SOL →
confidential flip_box → decrypt the box → assert perp == buy(...) from the transparent curve`.

**PASS = the confidential flip runs on real MPC and matches the public curve exactly** (the M0a
runtime gate). This is the proof we deferred on Windows.

---

## What's already done (so you know what to expect)
| Piece | State |
|---|---|
| 6 Arcis circuits (flip + custody + genesis) | compile; IDL matches design |
| M1 program (genesis + custody + flip) | builds to SBF |
| Curve math | proven exact (20M-input differential) + 200k-op solvency |
| SDK (`app/crates/flipvault-pathb-sdk`) | native + wasm, 5 tests |
| Validation test (`tests/flipvault-pathb.ts`) | ready — §11 runs it |
| VRF auto-selection, keeper, frontend | not built (next, after the flip validates) |

See `docs/FlipVault-pathb-STATUS.md` for the full status and `docs/FlipVault-pathb-backend-blueprint.md`
for the build plan.

## Gotchas already solved for you
- **Arcium crates = 0.11.1**, not 0.11.2 (CLI is ahead of crates.io). Already pinned in the repo.
- **io_uring/seccomp**: a Docker-on-Windows problem only — irrelevant on native Linux.
- **Don't use `cargo build-sbf` directly** (it hits a `getrandom` target error) — always `arcium build`.
- If the localnet validator ever times out on startup, raise `startup_wait` in `Anchor.toml`
  (native Linux is normally fine).

## Optional fast-path (skip the big downloads)
If this Linux box is also **Ubuntu 22.04** and you copy over the two staged tars from the Windows
machine (`C:\Users\Abcom\arcium-wsl-stage\arcium-bins.tar` = Solana+Anchor+arcium+arcup, and
`platform-tools.tar` = SBF tools), you can extract them instead of §4–§7/§9-download:
```bash
tar xf arcium-bins.tar -C "$HOME"        # ~/.cargo/bin/{anchor,arcium,arcup}, ~/.local/share/solana
# fix solana's active_release symlink to this home:
SI="$HOME/.local/share/solana/install"; ln -sfn "$(ls -d "$SI"/releases/*/solana-release|head -1)" "$SI/active_release"
mkdir -p "$HOME/.cache" && tar xf platform-tools.tar -C "$HOME/.cache"   # ~/.cache/solana/v1.52/platform-tools
```
You still need Docker (§2), Rust (§3), Node (§6), and `arcup install` to pull the Arx-node images.
