#!/usr/bin/env bash
set -euo pipefail

# herdr hands plugin commands a minimal PATH. Pick up the tools that mise manages too
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
PLUGIN_ID="k-narusawa.gh-review"

mode="${1:-open}"

# bash 3.2, the one macOS ships, expands "${VAR:-{\}}" to {\}, so the default goes on its own line
context="${HERDR_PLUGIN_CONTEXT_JSON:-}"
if [ -z "$context" ]; then
  context='{}'
fi

# Must work without jq. With no cwd to read, open in the current directory
cwd=""
if command -v jq >/dev/null 2>&1; then
  cwd="$(printf '%s' "$context" | jq -r '.focused_pane_cwd // empty' 2>/dev/null || true)"
fi
: "${cwd:=$PWD}"

case "$mode" in
  open)             target="" ;;
  review-requested) target="review-requested" ;;
  authored)         target="authored" ;;
  open-url)
    # The clicked URL arrives in its own env var (confirmed against the official browser plugin)
    target="${HERDR_PLUGIN_CLICKED_URL:-}"
    if [ -z "$target" ]; then
      echo "no PR URL was received" >&2
      exit 1
    fi
    ;;
  *)
    echo "usage: pane.sh {open|review-requested|authored|open-url}" >&2
    exit 1
    ;;
esac

args=(plugin pane open --plugin "$PLUGIN_ID" --entrypoint pane --placement tab --cwd "$cwd")
if [ -n "$target" ]; then
  args+=(--env "GH_REVIEW_TARGET=$target")
fi

"$HERDR" "${args[@]}"
