#requires -Version 5
# FlipVault dev helper — thin wrapper over `docker compose` for the Solana/Anchor toolchain.
# Usage examples:
#   ./dev.ps1 build
#   ./dev.ps1 up
#   ./dev.ps1 versions
#   ./dev.ps1 anchor build
#   ./dev.ps1 shell
param(
  [Parameter(Position = 0)][string]$Cmd = "help",
  [Parameter(Position = 1, ValueFromRemainingArguments = $true)][string[]]$Rest
)

$ErrorActionPreference = "Stop"
$svc = "dev"

function Compose { docker compose @args }

switch ($Cmd) {
  "build"         { Compose build }
  "up"            { Compose up -d }
  "down"          { Compose down }
  "shell"         { Compose exec $svc bash }
  "ps"            { Compose ps }
  "versions"      { Compose exec $svc bash -lc "rustc --version; solana --version; anchor --version; node --version" }
  "anchor"        { Compose exec $svc anchor @Rest }
  "cargo"         { Compose exec $svc cargo @Rest }
  "build-program" { Compose exec $svc anchor build }
  "test"          { Compose exec $svc anchor test }
  "validator"     { Compose exec $svc solana-test-validator --bind-address 0.0.0.0 --rpc-port 8899 @Rest }
  "logs"          { Compose logs -f $svc }
  "clean"         { Compose down -v }
  default {
    @"
FlipVault dev helper (Docker)

  ./dev.ps1 build           Build the toolchain image (slow first time)
  ./dev.ps1 up              Start the dev container (detached)
  ./dev.ps1 versions        Print rust / solana / anchor / node versions
  ./dev.ps1 shell           Open a bash shell inside the container
  ./dev.ps1 build-program   anchor build
  ./dev.ps1 test            anchor test
  ./dev.ps1 validator       Start solana-test-validator (RPC :8899, ws :8900)
  ./dev.ps1 anchor <args>   Run any anchor command in the container
  ./dev.ps1 cargo  <args>   Run any cargo command in the container
  ./dev.ps1 down            Stop the container
  ./dev.ps1 clean           Stop and DELETE volumes (full reset)
"@ | Write-Host
  }
}
