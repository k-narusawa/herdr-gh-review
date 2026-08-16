#!/usr/bin/env bash
set -euo pipefail

# herdr hands plugin commands a minimal PATH. Pick up the tools that mise manages too
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
AGENT="${GH_REVIEW_AI_CMD:-claude}"

repo="${1:?usage: ai.sh <repo> <pr> <outpath>}"
pr="${2:?}"
out="${3:?}"

mkdir -p "$(dirname "$out")"

# Written whole or not at all, so the 0.5s poll never reads a half-finished file
read -r -d '' prompt <<EOF || true
Review pull request #${pr} of ${repo} and write the result to ${out} as JSON.

1. Run /code-review ${pr}. Do not pass --comment or --fix — nothing may be posted to GitHub.
2. Turn the findings into this shape, write it to ${out}.tmp, then move that file to ${out}:
   {"body":"overall summary","comments":[{"path":"...","line":N,"side":"RIGHT","body":"..."}]}
   - path is relative to the repository root, line is the line number in the new file
   - body says what is wrong and how to fix it
3. Say only that you have written the file.

If no code-review skill is available, read the diff with \`gh pr diff ${pr}\`, review it
yourself, and write the same JSON.
EOF

# herdr pane split returns pane info as JSON; the new pane's id is at .result.pane.pane_id
pane_id="$("$HERDR" pane split --current --direction right --focus | jq -r '.result.pane.pane_id')"
"$HERDR" pane run "$pane_id" "$AGENT" "$prompt"
