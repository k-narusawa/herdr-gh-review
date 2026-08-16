# herdr-gh-review

Review GitHub pull requests in a [herdr](https://herdr.dev) pane, and **submit a real review to
GitHub** — line comments, a summary, Approve or Request changes. Read the diff, write the notes,
send it. You never leave the terminal.

Other herdr review panes point at your agent's diff and hand the notes back to the agent.
This one points at a pull request and posts to GitHub.

It also answers the question you actually start from — *what am I supposed to review?* — with
cross-repository lists of the PRs waiting on you and the PRs you opened.

## Requirements

- herdr 0.7.5 or newer
- [gh](https://cli.github.com/) 2.x, already authenticated (`gh auth login`)
- Rust 1.85 or newer (edition 2024). Development pins 1.97.1 via `mise.toml`
- [delta](https://github.com/dandavison/delta) (optional). If it is on `PATH`, diffs get syntax
  highlighting and word-level emphasis, using the `[delta]` section of your gitconfig as-is.
  Without it, diffs fall back to plain +/- colors

## Install

```bash
git clone https://github.com/k-narusawa/herdr-gh-review
herdr plugin link ./herdr-gh-review
bash ./herdr-gh-review/herdr/install.sh
```

## Usage

| Action | What it opens |
|---|---|
| `gh-review: open PR list` | Open PRs in the current repository |
| `gh-review: PRs awaiting my review` | PRs that requested your review, across repositories |
| `gh-review: my open PRs` | PRs you opened, across repositories — for reviewing yourself first |
| Click a PR URL | That pull request, directly |

Only `open PR list` looks at the current repository's remote; the others work from anywhere.

To launch it by hand, pass `--review-requested`, `--authored`, `--pr <number>`, or `--url <url>`.

A tree of the changed files sits to the left of the diff. The file under the cursor is
highlighted and follows you as you move with `}` / `{`. `T` toggles the tree, and terminals
narrower than 80 columns drop it automatically in favor of the diff.

`s` switches the diff between unified (one column) and split (old on the left, new on the right).
In split view, removed and added lines sit at the same height and a line that exists on only one
side leaves the other blank. Comments attach to the line in the cell under the cursor, so use
`h` / `l` to choose which side you are aiming at.

### Keys

| Key | Action |
|---|---|
| `j` / `k` | Move |
| `Ctrl-d` / `Ctrl-u` | Half-page move |
| `g` / `G` | Top / bottom |
| `}` / `{` | Next / previous file |
| `Tab` | Collapse a file |
| `T` | Toggle the file tree |
| `s` | Toggle split / unified |
| `h` / `l` | Move the cursor between cells in split view |
| `c` | Comment on the line under the cursor (opens `$EDITOR`) |
| `d` | Delete a comment |
| `D` | Discard comments that are no longer in the diff |
| `e` | Edit the review summary |
| `S` | Submit (Comment / Approve / Request changes) |
| `o` | Open the PR in a browser |
| `r` | Reload |
| `?` | Show the keys |
| `q` | Back / quit |

Reviews in progress are saved automatically, and cleared only once a submit succeeds.

## When the PR gets new commits

New commits change the diff, so some of the lines you commented on may no longer be in it.
When that happens the count appears in the status bar and in the submit dialog.

Those comments are not sent — the rest of the review goes through as usual. They stay in the
draft so you can rewrite them, and `D` discards them once you no longer need them.

## Development

Link once. You do not need to `unlink` and `link` again.

| What changed | What to do |
|---|---|
| `src/**.rs` | Run `bash herdr/install.sh`, then reopen the pane |
| `herdr/*.sh` | Nothing — the next launch picks it up |
| `herdr-plugin.toml` | Run `herdr plugin link .` (it overwrites an existing registration) |

Every pane `exec`s a fresh `bin/herdr-gh-review`, so replacing the binary is enough for the next
launch to run the new one. Only `herdr-plugin.toml` needs a re-link, because linking copies it
into `~/.config/herdr/plugins.json`.
