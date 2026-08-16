# herdr-gh-review

GitHubのPull Requestを [herdr](https://herdr.dev) のペイン内でレビューするプラグイン。
diffを読み、行コメントを書き、レビューとして提出するところまでをターミナルで完結させる。

## 必要なもの

- herdr 0.7.5 以上
- [gh](https://cli.github.com/) 2.x（`gh auth login` 済みであること）
- Rust 1.85 以上（edition 2024）。開発は `mise.toml` で 1.97.1 に固定
- [delta](https://github.com/dandavison/delta)（任意）。PATHにあればdiffに構文ハイライトと語単位の強調が付く。gitconfigの `[delta]` 設定（テーマ等）をそのまま使う。無ければ従来の +/- 色で表示する

## インストール

```bash
git clone https://github.com/k-narusawa/herdr-gh-review
herdr plugin link ./herdr-gh-review
bash ./herdr-gh-review/herdr/install.sh
```

## 開発

linkは一度だけでよい。`unlink` → `link` は不要。

| 変えたもの | やること |
|---|---|
| `src/**.rs` | `bash herdr/install.sh` してペインを開き直す |
| `herdr/*.sh` | そのまま次の起動から反映される |
| `herdr-plugin.toml` | `herdr plugin link .`（登録済みでも上書きされる） |

ペインは毎回 `bin/herdr-gh-review` を新しく exec するので、バイナリを差し替えれば次の起動から新しい方が動く。
`herdr-plugin.toml` だけは link 時に `~/.config/herdr/plugins.json` へ写しが作られるため、再linkが要る。

## 使い方

| アクション | 内容 |
|---|---|
| `gh-review: open PR list` | カレントリポジトリのopen PR一覧 |
| `gh-review: PRs awaiting my review` | 自分がレビューを依頼されているPR（リポジトリ横断） |
| `gh-review: my open PRs` | 自分が作成したPR（リポジトリ横断）。人に見せる前の自己レビュー用 |
| PR URLをクリック | そのPRを直接開く |

カレントリポジトリのリモートを見るのは `open PR list` だけで、他はどこから起動しても動く。
手で起動する場合は `--review-requested` / `--authored` / `--pr <番号>` / `--url <URL>` を渡す。

diff画面の左には変更ファイルのツリーが出る。カーソルのいるファイルが反転表示され、`}` / `{` で移動すると追従する。
`T` で表示/非表示を切り替えられる。端末幅が80桁未満のときはdiff本体を優先して自動的に出さない。

### キー操作

| キー | 動作 |
|---|---|
| `j` / `k` | 移動 |
| `Ctrl-d` / `Ctrl-u` | 半画面移動 |
| `g` / `G` | 先頭 / 末尾 |
| `}` / `{` | 次 / 前のファイル |
| `Tab` | ファイルの折りたたみ |
| `T` | 左のファイルツリーの表示切替 |
| `c` | カーソル行にコメント（`$EDITOR` が開く） |
| `d` | コメントを削除 |
| `e` | レビュー全体コメントを編集 |
| `S` | 提出（Comment / Approve / Request changes） |
| `o` | ブラウザでPRを開く |
| `r` | 再読み込み |
| `?` | キー一覧を表示 |
| `q` | 戻る / 終了 |

書きかけのレビューは自動保存される。提出に成功したときだけ消える。

## 既知の制限

PRに新しいコミットが積まれると、それ以前に書いた行コメントのうち、対象行が現在の差分に
存在しなくなったものは画面に表示されなくなります。下書きには残っており提出時には送られるため、
GitHubがレビュー全体を422で拒否します。この状態になると画面上部に警告が出ます。

現状の回避策は、下書きファイルを削除してコメントを書き直すことです:

```
rm ~/.local/state/herdr/plugins/k-narusawa.gh-review/drafts/<owner>-<repo>-<PR番号>.json
```
