# herdr-gh-review

GitHubのPull Requestを [herdr](https://herdr.dev) のペイン内でレビューするプラグイン。
diffを読み、行コメントを書き、レビューとして提出するところまでをターミナルで完結させる。

## 必要なもの

- herdr 0.7.5 以上
- [gh](https://cli.github.com/) 2.x（`gh auth login` 済みであること）
- Rust 1.97 以上（ビルドに必要）

## インストール

```bash
git clone https://github.com/k-narusawa/herdr-gh-review
herdr plugin link ./herdr-gh-review
bash ./herdr-gh-review/herdr/install.sh
```

## 使い方

| アクション | 内容 |
|---|---|
| `gh-review: open PR list` | カレントリポジトリのopen PR一覧 |
| `gh-review: PRs awaiting my review` | 自分がレビューを依頼されているPR |
| PR URLをクリック | そのPRを直接開く |

### キー操作

| キー | 動作 |
|---|---|
| `j` / `k` | 移動 |
| `Ctrl-d` / `Ctrl-u` | 半画面移動 |
| `g` / `G` | 先頭 / 末尾 |
| `}` / `{` | 次 / 前のファイル |
| `Tab` | ファイルの折りたたみ |
| `c` | カーソル行にコメント（`$EDITOR` が開く） |
| `d` | コメントを削除 |
| `e` | レビュー全体コメントを編集 |
| `S` | 提出（Comment / Approve / Request changes） |
| `q` | 戻る / 終了 |

書きかけのレビューは自動保存される。提出に成功したときだけ消える。
