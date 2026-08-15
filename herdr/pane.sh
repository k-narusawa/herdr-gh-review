#!/usr/bin/env bash
set -euo pipefail

PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
PLUGIN_ID="k-narusawa.gh-review"

mode="${1:-open}"

context="${HERDR_PLUGIN_CONTEXT_JSON:-{\}}"
cwd="$(printf '%s' "$context" | jq -r '.focused_pane_cwd // empty')"
: "${cwd:=$PWD}"

case "$mode" in
  open)             target="" ;;
  review-requested) target="review-requested" ;;
  open-url)
    # クリックされたURLは専用の環境変数で届く（公式browserプラグインの実装で確認済み）
    target="${HERDR_PLUGIN_CLICKED_URL:-}"
    if [ -z "$target" ]; then
      echo "PRのURLを受け取れませんでした" >&2
      exit 1
    fi
    ;;
  *)
    echo "usage: pane.sh {open|review-requested|open-url}" >&2
    exit 1
    ;;
esac

args=(plugin pane open --plugin "$PLUGIN_ID" --entrypoint pane --placement tab --cwd "$cwd")
if [ -n "$target" ]; then
  args+=(--env "GH_REVIEW_TARGET=$target")
fi

"$HERDR" "${args[@]}"
