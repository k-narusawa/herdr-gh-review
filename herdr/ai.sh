#!/usr/bin/env bash
set -euo pipefail

# herdr hands plugin commands a minimal PATH. Pick up the tools that mise manages too
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:$PATH"
HERDR="${HERDR_BIN_PATH:-herdr}"
AGENT="${GH_REVIEW_AI_CMD:-claude}"

# The diff under review is written by whoever opened the pull request, so everything the agent
# reads is untrusted input. These are denied at the tool layer, which outranks the permission
# mode, so the prompt's prohibitions stop being requests the agent may be talked out of.
#
# ponytail: a deny list is a floor, not a ceiling — `python3 -c` still reaches the network.
# Closing that needs a sandbox with no egress, which is beyond what this plugin can set up
DENY="Bash(gh pr comment:*),Bash(gh pr review:*),Bash(gh pr edit:*),Bash(gh pr merge:*)"
DENY="$DENY,Bash(gh pr close:*),Bash(gh pr ready:*),Bash(gh issue comment:*),Bash(gh api:*)"
DENY="$DENY,Bash(git push:*),Bash(git commit:*),Bash(rm:*)"
DENY="$DENY,Bash(curl:*),Bash(wget:*),Bash(nc:*),Bash(ssh:*),Bash(scp:*)"
DENY="$DENY,WebFetch,WebSearch"

# Pre-approved so the review itself never stops to ask. Everything outside this list — editing
# a file anywhere but the handoff directory above all — still prompts, and that prompt is the
# only thing that catches an injected instruction nobody thought to deny.
# Edit(path) is the rule form that covers every file-writing tool; Write(path) is ignored
ALLOW="Bash(gh pr diff:*),Bash(gh pr view:*),Bash(git diff:*),Bash(git log:*)"
ALLOW="$ALLOW,Bash(git show:*),Bash(git blame:*),Read,Grep,Glob"

# herdr pane run types its command into the pane's shell instead of exec'ing argv, so a prose
# prompt cannot be passed as an argument — the shell would split and reparse it. The pane runs
# this branch instead, and the prompt reaches the agent as one argv element, never via a shell
if [ "${1:-}" = "--exec" ]; then
  agent="$2"
  # No --permission-mode: whatever the agent wants to do beyond the review stops and asks, which
  # is the only thing that catches an injected instruction nobody thought to put on the deny list
  if [ "$(basename "$agent")" = "claude" ]; then
    # The prompt writes .tmp and renames it, so exactly that one rename is pre-approved and
    # nothing else. `Bash(mv:*)` would have paired with the Edit rule below into an unprompted
    # write to any path on the machine — write here, move there
    out="${3%.prompt}.json"
    # An absolute path in a permission rule takes a doubled leading slash
    exec "$agent" --allowedTools "$ALLOW,Edit(/$(dirname "$3")/**),Bash(mv $out.tmp $out)" \
      --disallowedTools "$DENY" -- "$(cat "$3")"
  fi
  exec "$agent" "$(cat "$3")"
fi

# Leaving the diff view closes the panes it opened. Called with the file ai.sh appended their
# ids to; a pane the user already closed by hand just fails, which is fine
if [ "${1:-}" = "--close" ]; then
  [ -f "$2" ] || exit 0
  while read -r id; do
    [ -n "$id" ] && "$HERDR" pane close "$id" >/dev/null 2>&1 || true
  done <"$2"
  rm -f "$2" "${2%.panes}.prompt"
  exit 0
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

# Recorded so leaving the diff view can close what it opened. One line per press of A
printf '%s\n' "$pane_id" >>"${out%.json}.panes"

# Every argument is quoted for the pane's shell, which re-parses this line. $AGENT comes from
# GH_REVIEW_AI_CMD, so leaving it bare would let a value with a space or a `;` split or run
"$HERDR" pane run "$pane_id" bash "$(printf '%q' "$0")" --exec "$(printf '%q' "$AGENT")" \
  "$(printf '%q' "$prompt_file")"
