#!/usr/bin/env bash
set -euo pipefail

# herdr はプラグインコマンドに最小限の PATH しか渡さない。mise で管理しているツールも拾う
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
PLUGIN_ID="k-narusawa.gh-review"

mode="${1:-open}"

# macOS 標準の bash 3.2 は "${VAR:-{\}}" を {\} に展開してしまうので、既定値は別行で入れる
context="${HERDR_PLUGIN_CONTEXT_JSON:-}"
if [ -z "$context" ]; then
  context='{}'
fi

# jq が無くても動くこと。cwd が取れなければカレントディレクトリで開く
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
    # クリックされたURLは専用の環境変数で届く（公式browserプラグインの実装で確認済み）
    target="${HERDR_PLUGIN_CLICKED_URL:-}"
    if [ -z "$target" ]; then
      echo "PRのURLを受け取れませんでした" >&2
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
