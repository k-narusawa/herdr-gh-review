# herdr-gh-review 設計書

作成日: 2026-08-15

## 目的

GitHubのPull Requestを、herdrのペイン内で読み・行コメントを書き・レビューとして提出できるようにする
herdrプラグイン。ターミナルから出ずにレビューを完結させることが狙い。

将来的にAIエージェントと協働するレビュー環境へ発展させるが、v1のスコープには含めない（後述）。

## v1のスコープ

含むもの:

- PR一覧の表示（2種類の入口）
  - 現在のワークスペースのリポジトリのopen PR
  - 自分がレビューを依頼されているPR（リポジトリ横断）
- PR URLからの直接オープン（herdrのlink handler経由）
- 変更差分の閲覧（全ファイルを縦に連結した1画面スクロール）
- 行単位のコメントの作成・削除
- レビュー全体コメントの作成
- GitHubへのレビュー提出（Comment / Approve / Request changes）
- 下書きコメントのローカル永続化

含まないもの（意図的に外す）:

- 既存のレビューコメント・会話スレッド・PR本文の表示
  - v1は「読む→書く→提出」の本線を最短で動かすことを優先する。既存コメントの表示は
    行との対応付け処理が増えるため、本線が動いてから追加する。
- シンタックスハイライト
  - 依存が重くなる割に、diffを読む上での効果は追加/削除の色分けほど大きくない。
- AIエージェント連携
  - 第2段階として扱う。ただし接続点だけは設計に残す（「AI連携への接続点」節）。
- PRのマージ、レビューの取り消し、既存コメントへの返信
- Windowsサポート（herdrがmacOS/Linuxのみのため）

## 全体構成

herdrプラグインは「マニフェスト（TOML）＋任意のCLIプログラム」という構造をとる。本プラグインの
実体は `herdr-gh-review` という単一のRustバイナリであり、herdrはそれをペイン内で起動するだけ。

```
herdr-gh-review/
├── herdr-plugin.toml     # herdrへの登録情報
├── herdr/
│   ├── install.sh        # plugin install 時のビルド
│   └── pane.sh           # ペインのopen/close/toggle
└── src/
    ├── main.rs           # 引数解析・画面遷移
    ├── gh.rs             # ghコマンド呼び出しの集約
    ├── diff.rs           # unified diff のパース
    ├── review.rs         # 下書きコメントの保持・永続化・APIリクエスト組み立て
    ├── app.rs            # アプリケーション状態
    └── ui/
        ├── prlist.rs     # PR一覧画面
        ├── diffview.rs   # diff閲覧画面
        └── submit.rs     # 提出ダイアログ
```

実装言語はRust、TUIライブラリはratatui。同じ構成の先行プラグイン `persiyanov.reviewr` が
ローカルにインストール済みで、herdr連携部分の参照実装として使える。

### プラグインマニフェスト

```toml
id = "k-narusawa.gh-review"
name = "gh-review"
version = "0.1.0"
min_herdr_version = "0.7.5"
platforms = ["macos", "linux"]
description = "Review GitHub pull requests in a herdr pane."

[[build]]
command = ["bash", "herdr/install.sh"]

[[panes]]
id = "pane"
title = "gh-review"
placement = "tab"
command = ["sh", "-c", "exec \"$HERDR_PLUGIN_ROOT/bin/herdr-gh-review\""]

[[actions]]
id = "open"
title = "gh-review: open PR list"
contexts = ["pane", "workspace"]
command = ["bash", "herdr/pane.sh", "open"]

[[actions]]
id = "review-requested"
title = "gh-review: PRs awaiting my review"
contexts = ["pane", "workspace"]
command = ["bash", "herdr/pane.sh", "review-requested"]

[[link_handlers]]
id = "pr-url"
title = "Open in gh-review"
pattern = "^https://github\\.com/[^/]+/[^/]+/pull/[0-9]+"
action = "open-url"

[[actions]]
id = "open-url"
title = "gh-review: open PR URL"
contexts = ["workspace"]
command = ["bash", "herdr/pane.sh", "open-url"]
```

配置を `tab` にしたのは、全ファイル連結スクロール表示が縦に長く、分割ペインの幅では読みにくい
ため。`herdr/pane.sh` 内で設定により `split` へ変更できるようにする。

ペインのコマンドは固定であり、アクションごとの引数を渡せない。表示対象は環境変数
`GH_REVIEW_TARGET` で受け渡す。`herdr plugin pane open` は `--env KEY=VALUE` を受け付ける
（herdr 0.7.5で確認済み）ため、`pane.sh` が以下のように指定する。

| アクション | 渡す値 |
|---|---|
| `open` | 未設定（カレントリポジトリのopen PR一覧） |
| `review-requested` | `GH_REVIEW_TARGET=review-requested` |
| `open-url` | `GH_REVIEW_TARGET=<PR URL>` |

### herdrから受け取る環境変数

herdrはプラグインのコマンドとペインに以下を渡す（herdr 0.7.5で確認済み）。本プラグインが読むのは:

- `HERDR_PLUGIN_ROOT` — バイナリの絶対パス解決に使う。ペインのcwdはリポジトリであってプラグイン
  ルートではないため、相対パスでは起動できない。
- `HERDR_PLUGIN_STATE_DIR` — 下書きの保存先（`~/.local/state/herdr/plugins/<plugin_id>/`）。
- `HERDR_PLUGIN_CONFIG_DIR` — 設定ファイルの読み込み先。未設定時は
  `herdr plugin config-dir <plugin_id>` にフォールバックする。
- `HERDR_PLUGIN_CONTEXT_JSON` — アクション起動時のコンテキスト。`pane.sh` がペインを開く
  ディレクトリの決定に使う。

## GitHubとのやりとり

HTTPクライアントは自前で持たず、`gh` コマンドを子プロセスとして呼ぶ。認証・トークン更新・
GitHub Enterprise対応をすべて `gh` に委譲できるため。呼び出しは `gh.rs` に集約する。

| 目的 | コマンド |
|---|---|
| リポジトリのopen PR一覧 | `gh pr list --json number,title,author,additions,deletions,isDraft,updatedAt` |
| レビュー依頼一覧 | `gh search prs --review-requested=@me --state=open --json repository,number,title,author,updatedAt` |
| PRのメタ情報 | `gh pr view <n> --json number,title,author,additions,deletions,headRefOid,url` |
| 差分の取得 | `gh pr diff <n>` |
| レビュー提出 | `gh api --method POST repos/{owner}/{repo}/pulls/{n}/reviews --input -` |

リポジトリを跨ぐ操作（レビュー依頼一覧から開く場合）は `--repo <owner>/<repo>` を明示する。

### レビュー提出のリクエスト形式

`POST /repos/{owner}/{repo}/pulls/{number}/reviews` に以下のJSONを標準入力から渡す。

```json
{
  "commit_id": "<headRefOid>",
  "event": "COMMENT",
  "body": "レビュー全体のコメント",
  "comments": [
    { "path": "src/auth/login.ts", "line": 15, "side": "RIGHT", "body": "null時に401を返すべきでは？" }
  ]
}
```

- `event` は `COMMENT` / `APPROVE` / `REQUEST_CHANGES` のいずれか。
- `line` は対象ファイルの行番号、`side` はどちら側の行かを表す。
  - 追加行（`+`）および変更なし行（` `）→ `side: "RIGHT"`、`line` は**変更後**の行番号
  - 削除行（`-`）→ `side: "LEFT"`、`line` は**変更前**の行番号
- 旧APIの `position`（diff先頭からの相対位置）は使わない。
- `body` が空文字の場合、`APPROVE` は許容されるが `REQUEST_CHANGES` はGitHubに拒否される。
  提出ダイアログ側で `REQUEST_CHANGES` 選択時に本文必須とする。
- **diffに含まれない行にコメントするとGitHubは422を返す。** TUI側でコメント可能な行を
  diff中の行のみに限定することで、この状態を発生させない。

## データモデル

`diff.rs` が `gh pr diff` の出力を以下の構造に変換する。表示とコメント対象の解決の両方が
この構造だけで完結する。

```
ParsedDiff { files: Vec<FileDiff> }

FileDiff {
    old_path: Option<String>,   // 追加ファイルならNone
    new_path: Option<String>,   // 削除ファイルならNone
    is_binary: bool,
    hunks: Vec<Hunk>,
    additions: usize,
    deletions: usize,
}

Hunk {
    header: String,             // "@@ -12,7 +12,9 @@ fn login()"
    lines: Vec<DiffLine>,
}

DiffLine {
    kind: Added | Removed | Context,
    old_lineno: Option<u32>,    // Added の場合はNone
    new_lineno: Option<u32>,    // Removed の場合はNone
    text: String,
}
```

`DiffLine` から提出用の `(path, line, side)` は一意に決まる:

- `Added` / `Context` → `(new_path, new_lineno, RIGHT)`
- `Removed` → `(old_path, old_lineno, LEFT)`

バイナリファイルとリネームのみのファイルは表示するがコメント不可とする。

unified diffのパースは既存クレート（`diffy` 等）で賄えるか最初に確認し、必要な行番号情報が
取れない場合のみ自前実装する。自前実装する場合もハンクヘッダ `@@ -a,b +c,d @@` から
両側の行番号を進めるだけであり、規模は小さい。

### 下書きコメント

```
Draft {
    repo: String,          // "owner/repo"
    pr_number: u32,
    head_sha: String,      // 取得時のheadRefOid
    body: String,          // レビュー全体コメント
    comments: Vec<DraftComment>,
}

DraftComment { path: String, line: u32, side: Side, body: String }
```

`HERDR_PLUGIN_STATE_DIR/drafts/<owner>-<repo>-<pr>.json` に保存する。コメントの追加・編集・
削除のたびに書き出す（ファイルは小さく、書き込み頻度は人間の操作速度に律速されるため
バッファリングは不要）。

PRを開いた際、保存済み下書きがあれば読み込む。ただし保存時の `head_sha` が現在のものと
異なる場合、PRに新しいコミットが積まれており行番号がずれている可能性がある。この場合は
下書きを破棄せず、警告を表示した上で読み込み、ユーザーが提出前に確認できるようにする。

提出成功後、対応する下書きファイルを削除する。

## 画面と操作

画面は2つ。PR一覧 → Enter → diff閲覧。diff閲覧から `q` で一覧に戻る。

### PR一覧画面

起動時の指定で表示内容が決まる。herdrのペインからは環境変数 `GH_REVIEW_TARGET` で、手動で
バイナリを実行する場合はコマンドライン引数で指定する（引数が優先）。

| 指定 | 表示 |
|---|---|
| なし | カレントリポジトリのopen PR一覧 |
| `review-requested` / `--review-requested` | 自分宛のレビュー依頼一覧（リポジトリ名を併記） |
| PR URL / `--url <url>` | 一覧を飛ばして直接diff閲覧を開く |
| `--pr <n>` | 同上（カレントリポジトリのPR番号指定） |

各行に PR番号・タイトル・作者・`+追加 -削除`・Draft表示を出す。

### diff閲覧画面

全ファイルのdiffを縦に連結して表示する。各ファイルの先頭にヘッダ行（パスと増減行数）を置き、
折りたたみ可能とする。追加行は緑、削除行は赤、変更なし行は通常色。行番号は旧・新を並べて表示する。
下書きコメントは対象行の直下にインラインで表示する。

キー割り当て:

| キー | 動作 |
|---|---|
| `j` / `k` | 1行移動 |
| `Ctrl-d` / `Ctrl-u` | 半画面移動 |
| `g` / `G` | 先頭 / 末尾 |
| `}` / `{` | 次 / 前のファイル |
| `Tab` | カーソル位置のファイルの折りたたみ切り替え |
| `c` | カーソル行にコメント（`$EDITOR` を起動） |
| `d` | カーソル行のコメントを削除 |
| `e` | レビュー全体コメントを編集（`$EDITOR` を起動） |
| `S` | 提出ダイアログを開く |
| `o` | ブラウザでPRを開く（`gh pr view --web`） |
| `r` | PRを再取得して再描画 |
| `q` | PR一覧に戻る / 一覧では終了 |
| `?` | ヘルプ表示 |

コメント本文の入力は `$EDITOR`（未設定時は `vi`）を起動して行う。起動前にターミナルの
raw modeとalternate screenを解除し、終了後に復帰する。一時ファイルの内容が空、または
エディタが非ゼロ終了した場合はコメントを作成しない。

### 提出ダイアログ

`Comment` / `Approve` / `Request changes` を選択し、確認して送信する。送信内容の要約
（コメント件数、全体コメントの有無）を表示する。`REQUEST_CHANGES` を選んで全体コメントが
空の場合は、送信せずに全体コメントの入力を促す。

## エラー処理

`gh` の呼び出しは失敗しうる。失敗時はTUIを壊さず、画面下部のステータス行に1行のメッセージを
表示する。想定する主な失敗:

| 状況 | 扱い |
|---|---|
| `gh` が見つからない | 起動時に検出し、インストール手順を示して終了する |
| 未認証（`gh auth status` が失敗） | 起動時に検出し、`gh auth login` を促して終了する |
| カレントディレクトリがGitHubリポジトリでない | PR一覧が空である旨とレビュー依頼一覧への切り替えを案内する |
| PR取得失敗・ネットワークエラー | ステータス行にエラーを表示し、`r` での再試行を可能にする |
| レビュー提出失敗（422等） | **下書きを削除せず保持したまま** エラーを表示する。レビュー内容を失わないことを最優先する |

`gh` は失敗時に標準エラー出力へメッセージを出す。ステータス行は1行のため、全文はログ
（`HERDR_PLUGIN_STATE_DIR/log`）へ書き、画面には要約を出す。

herdrがプラグインコマンドに渡す `PATH` は最小限であるため、`gh` および `git` の探索は
`PATH` に一般的なbinディレクトリ（`/opt/homebrew/bin`, `~/.local/bin`, mise のshim）を
補ってから行う。

## テスト方針

外部コマンド呼び出しを含むため、テスト対象を純粋なロジックに絞る。

- `diff.rs` — unified diffの文字列を入力し、パース結果を検証する
  - 追加のみのファイル / 削除のみのファイル / 複数ハンク / リネーム / バイナリファイル /
    ファイル末尾の改行なし（`\ No newline at end of file`）
  - 各 `DiffLine` の `old_lineno` / `new_lineno` が正しく進むこと
- `review.rs` — 下書きから提出用JSONを組み立てる関数を検証する
  - `side` の出し分け（追加行→RIGHT、削除行→LEFT）
  - 全体コメントのみ / 行コメントのみ / 両方 / 空のケース
  - 下書きJSONのシリアライズとデシリアライズの往復

`gh.rs` はモックせず、手動で動作確認する。ここをテスト可能にする抽象化は、得られる保証に対して
構造の増加が見合わない。

## AI連携への接続点

第2段階でAIエージェントとの協働レビューに発展させる。herdr側のAPIは既に揃っている:

- `herdr agent list` — 同一ワークスペースのエージェント（claude等）を列挙する
- `herdr pane send-text <pane_id> "<text>"` — エージェントの入力欄にテキストを流し込む（Enterは送らない）

`review.rs` が保持する下書きは `{path, line, side, body}` の配列であり、これをMarkdownに整形して
`send-text` に渡すアクションを追加すれば連携が成立する。

v1で守るべき制約はひとつだけ:

- **下書きの型をGitHub API都合で歪めない。** `path` / `line` / `side` / `body` に、
  APIリクエストの組み立て以外の意味を持たせない。

これ以外に、AI連携のための前倒し実装は行わない。

## 未確定事項

以下は実装時に確認して決める。設計判断には影響しない。

- `diffy` クレートが `DiffLine` に必要な行番号情報を提供するか。しない場合は自前パーサとする。
- 巨大PR（数千行のdiff）での描画性能。問題が出た場合は表示行のウィンドウ化で対応する。
