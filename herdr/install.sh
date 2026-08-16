#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# herdr はビルド手順にも最小限の PATH しか渡さない。mise で管理している cargo を拾う。
# Homebrew に古い cargo が入っていることがあるので、mise の shims を先に置く
PATH="$HOME/.local/share/mise/shims:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

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
