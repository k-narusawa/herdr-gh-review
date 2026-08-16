#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# herdr hands even the build step a minimal PATH. Pick up the cargo that mise manages.
# Homebrew sometimes carries an older cargo, so mise's shims go first
PATH="$HOME/.local/share/mise/shims:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# ponytail: builds from source, so a Rust toolchain is required.
# To ship this to other people, fetch a prebuilt binary from Releases instead
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found — install Rust from https://rustup.rs/" >&2
  exit 1
fi

cargo build --release
mkdir -p bin
cp target/release/herdr-gh-review bin/
echo "installed: $(pwd)/bin/herdr-gh-review"
