#!/usr/bin/env bash
set -euo pipefail

# herdr hands plugin commands a minimal PATH. Pick up the tools that mise manages too
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
AGENT="${GH_REVIEW_AI_CMD:-claude}"

# Denied at the tool layer, so the prompt's "never post to GitHub" stops being a request the
# agent may reinterpret. Deny beats the permission mode, so this holds whatever the user's
# settings say — and it is what stops a code-review skill whose last step posts a comment
DENY="Bash(gh pr comment:*),Bash(gh pr review:*),Bash(gh pr edit:*),Bash(gh pr merge:*)"
DENY="$DENY,Bash(gh pr close:*),Bash(gh pr ready:*),Bash(gh issue comment:*),Bash(gh api:*)"

# herdr pane run types its command into the pane's shell instead of exec'ing argv, so a prose
# prompt cannot be passed as an argument — the shell would split and reparse it. The pane runs
# this branch instead, and the prompt reaches the agent as one argv element, never via a shell
if [ "${1:-}" = "--exec" ]; then
  agent="$2"
  # dontAsk so the pane runs unattended whatever defaultMode the user set — `plan` would
  # otherwise leave the agent unable to write the file at all
  if [ "$(basename "$agent")" = "claude" ]; then
    exec "$agent" --permission-mode dontAsk --disallowedTools "$DENY" -- "$(cat "$3")"
  fi
  exec "$agent" "$(cat "$3")"
fi

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
   Do not write to GitHub in any way: no 'gh pr comment', no 'gh pr review', no 'gh api' write
   calls. Do not pass --comment or --fix, and do not modify the working tree. If the review
   procedure you are following ends by posting a comment, stop before that step and write the
   JSON file instead.
2. Turn the findings into this shape, write it to ${out}.tmp, then move that file to ${out}
   (written whole or not at all, so the 0.5s poll on the other end never reads a half-finished
   file):
   {"body":"overall summary","comments":[{"path":"...","line":N,"side":"RIGHT","body":"..."}]}
   - path is relative to the repository root, line is the line number in the new file
   - body says what is wrong and how to fix it
3. Say only that you have written the file.

If no code-review skill is available, read the diff with 'gh pr diff ${pr}', review it
yourself, and write the same JSON. Still never write to GitHub or modify the working tree.
EOF

prompt_file="${out%.json}.prompt"
printf '%s' "$prompt" >"$prompt_file"

# --cwd "$PWD" pins the new pane to this process's directory instead of relying on split
# inheriting it, so it is always the same repo the Rust side's current_repo() guard checked
# herdr pane split returns pane info as JSON; the new pane's id is at .result.pane.pane_id
pane_id="$("$HERDR" pane split --current --direction right --no-focus --cwd "$PWD" | jq -r '.result.pane.pane_id')"

# The paths are quoted for the pane's shell, which is what reads this line
"$HERDR" pane run "$pane_id" bash "$(printf '%q' "$0")" --exec "$AGENT" "$(printf '%q' "$prompt_file")"
