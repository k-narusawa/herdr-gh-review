#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# ponytail: ソースからビルドする（Rustツールチェーンが要る）。
# 他人に配るなら reviewr のように Releases のビルド済みバイナリを取りに行く形へ移す
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo が見つかりません。https://rustup.rs/ でRustを入れてください" >&2
  exit 1
fi

cargo build --release
mkdir -p bin
cp target/release/herdr-gh-review bin/
echo "installed: $(pwd)/bin/herdr-gh-review"
