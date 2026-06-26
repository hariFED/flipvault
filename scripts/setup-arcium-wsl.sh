#!/usr/bin/env bash
# FlipVault Path-B — set up the Arcium toolchain NATIVELY in WSL2 (Ubuntu-22.04).
#
# Why WSL2-native (not the Docker container): `arcium localnet` starts the MPC node containers via
# Docker. Run from inside our container it passes container-internal paths the host daemon can't
# resolve (Docker-out-of-Docker mismatch). Run from WSL2 with Docker Desktop integration, the paths
# align and the MPC nodes start.
#
# Reuses cached artifacts from the container (no ~1hr platform-tools download, no 26-min Anchor build):
#   /mnt/c/Users/Abcom/arcium-wsl-stage/arcium-bins.tar    (anchor + arcium + arcup + solana)
#   /mnt/c/Users/Abcom/arcium-wsl-stage/platform-tools.tar (SBF platform-tools v1.52)
#
# PREREQUISITE (one-time, in Docker Desktop GUI — cannot be scripted):
#   Settings -> Resources -> WSL Integration -> enable "Ubuntu-22.04" -> Apply & Restart.
#
# Run:  wsl -d Ubuntu-22.04        # then, inside:
#       bash /mnt/c/Users/Abcom/flipsol/scripts/setup-arcium-wsl.sh
set -euo pipefail

STAGE=/mnt/c/Users/Abcom/arcium-wsl-stage
say() { printf '\n\033[1;36m== %s\033[0m\n' "$*"; }

say "1/7 system deps (needs sudo once)"
sudo apt-get update -y
sudo apt-get install -y --no-install-recommends \
  curl git build-essential pkg-config libssl-dev libudev-dev ca-certificates xz-utils bzip2

say "2/7 Rust (rustup)"
if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.92.0 --profile minimal
fi
. "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown || true

say "3/7 extract cached Solana + Anchor + arcium binaries"
[ -f "$STAGE/arcium-bins.tar" ] || { echo "MISSING $STAGE/arcium-bins.tar"; exit 1; }
mkdir -p "$HOME/.cargo/bin"
tar xf "$STAGE/arcium-bins.tar" -C "$HOME"   # -> ~/.cargo/bin/{anchor,arcium,arcup}, ~/.local/share/solana
chmod +x "$HOME/.cargo/bin/anchor" "$HOME/.cargo/bin/arcium" "$HOME/.cargo/bin/arcup" 2>/dev/null || true
# The solana `active_release` symlink is absolute -> the container's /root home; re-point it here.
SOLINST="$HOME/.local/share/solana/install"
SOLREL="$(ls -d "$SOLINST"/releases/*/solana-release 2>/dev/null | head -1)"
[ -n "$SOLREL" ] && ln -sfn "$SOLREL" "$SOLINST/active_release"

say "4/7 extract cached SBF platform-tools (skips the ~1hr download)"
[ -f "$STAGE/platform-tools.tar" ] || { echo "MISSING $STAGE/platform-tools.tar"; exit 1; }
mkdir -p "$HOME/.cache"
tar xf "$STAGE/platform-tools.tar" -C "$HOME/.cache"   # -> ~/.cache/solana/v1.52/platform-tools

say "5/7 Node 22 + yarn (via nvm, no sudo)"
if [ ! -d "$HOME/.nvm" ]; then
  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
fi
export NVM_DIR="$HOME/.nvm"; . "$NVM_DIR/nvm.sh"
nvm install 22 && nvm alias default 22
corepack enable || npm i -g yarn

say "6/7 PATH + wallet"
LINE_CARGO='. "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"'
LINE_SOL='export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"'
grep -qF "$LINE_SOL" "$HOME/.bashrc" 2>/dev/null || printf '\n%s\n%s\n' "$LINE_CARGO" "$LINE_SOL" >> "$HOME/.bashrc"
export PATH="$HOME/.cargo/bin:$HOME/.local/share/solana/install/active_release/bin:$PATH"
[ -f "$HOME/.config/solana/id.json" ] || solana-keygen new --no-bip39-passphrase -o "$HOME/.config/solana/id.json"

say "7/7 verify"
echo "arcium : $(arcium --version 2>&1)"
echo "solana : $(solana --version 2>&1)"
echo "anchor : $(anchor --version 2>&1)"
echo "rustc  : $(rustc --version 2>&1)"
echo "node   : $(node --version 2>&1)"
echo -n "docker : "; (command -v docker >/dev/null && docker version --format '{{.Server.Version}}' 2>&1) || echo "MISSING — enable Docker Desktop WSL integration for Ubuntu-22.04"

cat <<'NEXT'

==========================================================================
 Toolchain ready. Next:
 1) Confirm Docker works here:  docker version   (if MISSING, enable
    Docker Desktop -> Settings -> Resources -> WSL Integration -> Ubuntu-22.04)
 2) Copy the project to the fast WSL filesystem (localnet's RocksDB ledger
    is slow on /mnt/c):
        cp -r /mnt/c/Users/Abcom/flipsol/path-b ~/path-b
        cd ~/path-b
 3) Run the live confidential flip:
        arcium test
    Expected: the decrypted box equals buy(...) from the transparent curve.
==========================================================================
NEXT
