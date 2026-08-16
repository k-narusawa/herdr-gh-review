#!/usr/bin/env bash
set -euo pipefail

# herdr hands plugin commands a minimal PATH. Pick up the tools that mise manages too
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
AGENT="${GH_REVIEW_AI_CMD:-claude}"

repo="${1:?usage: ai.sh <repo> <pr> <outpath>}"
pr="${2:?}"
out="${3:?}"

# herdr pane split's pane id comes back as JSON, not a plain line
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required (install it, e.g. \`brew install jq\`)" >&2
  exit 1
fi

mkdir -p "$(dirname "$out")"

read -r -d '' prompt <<EOF || true
Review pull request #${pr} of ${repo} and write the result to ${out} as JSON.

1. Run Claude Code's built-in code-review skill for PR #${pr} (the one that takes a PR number
   and returns findings — not a plugin whose last step posts a comment to GitHub).
   Do not write to GitHub in any way: no \`gh pr comment\`, no \`gh pr review\`, no \`gh api\`
   write calls. If the review procedure you are following ends by posting a comment, stop
   before that step and write the JSON file instead.
2. Turn the findings into this shape, write it to ${out}.tmp, then move that file to ${out}
   (written whole or not at all, so the 0.5s poll on the other end never reads a half-finished
   file):
   {"body":"overall summary","comments":[{"path":"...","line":N,"side":"RIGHT","body":"..."}]}
   - path is relative to the repository root, line is the line number in the new file
   - body says what is wrong and how to fix it
3. Say only that you have written the file.

If no code-review skill is available, read the diff with \`gh pr diff ${pr}\`, review it
yourself, and write the same JSON. Still never write to GitHub.
EOF

# herdr pane split returns pane info as JSON; the new pane's id is at .result.pane.pane_id
pane_id="$("$HERDR" pane split --current --direction right --no-focus | jq -r '.result.pane.pane_id')"
"$HERDR" pane run "$pane_id" "$AGENT" "$prompt"
