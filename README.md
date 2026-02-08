# bs

[blogsync](https://github.com/x-motemen/blogsync) のRust製ドロップイン代替。設定ファイルもエントリもそのまま、起動3倍速、CPU使用量1/3。

## クイックスタート

```sh
cargo install --path .
```

既存の blogsync 設定がそのまま使える:

```sh
bs pull                  # 全ブログの記事をダウンロード
bs push entry.md         # 編集した記事をアップロード
bs post blog.example.com --title "Hello" < body.md  # 新規投稿
```

## なぜ bs か

blogsync との完全互換を保ちつつ、Rust で書き直した。

| | bs | blogsync |
|---|---|---|
| 起動速度 | **1.9 ms** | 5.8 ms |
| CPU使用量 (2489記事 pull) | **0.09 s** | 0.32 s |
| バイナリサイズ | **3.4 MB** | 12 MB |

> [hyperfine](https://github.com/sharkdp/hyperfine) で計測、Apple Silicon。pull の実行時間差はネットワーク I/O が支配的なため小さい (26.0s vs 27.2s)。

**互換性:** 設定ファイル、エントリ形式、コマンド体系が完全互換。`blogsync` をアンインストールして `bs` を入れるだけ。

## コマンド

### pull (alias: pl)

リモートからエントリを一括ダウンロード。ローカルがリモートより新しいファイルはスキップする。

```sh
bs pull                            # 設定済み全ブログ
bs pull blog.example.com           # 特定ブログ
bs pull --only-drafts              # 下書きのみ
bs pull --no-drafts                # 下書き除外
```

### push

ローカルのエントリをリモートに反映。

```sh
bs push path/to/entry.md           # 更新
bs push path/to/entry.md --publish # 下書きを公開
```

### post

標準入力から新規エントリを作成。

```sh
echo "本文" | bs post blog.example.com --title "タイトル"
echo "下書き" | bs post blog.example.com --title "WIP" --draft
echo "About" | bs post blog.example.com --title "About" --page
echo "本文" | bs post blog.example.com --custom-path "my-entry"
```

### fetch

ローカルのエントリをリモートの最新版で更新。

```sh
bs fetch path/to/entry.md
```

### remove (alias: rm)

エントリをリモートとローカルの両方から削除。

```sh
bs remove path/to/entry.md
```

### list (alias: ls)

設定済みブログの一覧を表示。

```sh
bs list
```

### グローバルオプション

```sh
bs -C /path/to/workdir pull        # 作業ディレクトリを指定
```

## 設定

blogsync と同じ YAML 形式。

**検索順序:**
1. `./blogsync.yaml`
2. `~/.config/blogsync/config.yaml`

```yaml
default:
  local_root: ~/blog

blog.example.com:
  username: your-hatena-id
  password: your-api-key
```

環境変数でも設定できる:

| 環境変数 | 説明 |
|---|---|
| `BLOGSYNC_USERNAME` | はてなID |
| `BLOGSYNC_PASSWORD` | APIキー |
| `BLOGSYNC_WORKDIR` | 作業ディレクトリ (`-C` と同等) |

### ブログごとの設定項目

| 項目 | 必須 | 説明 |
|---|---|---|
| `username` | Yes | はてなID |
| `password` | Yes | APIキー |
| `local_root` | Yes | ローカルルートディレクトリ (`~` 展開対応) |
| `omit_domain` | No | `true` にするとパスからドメインを省略 (default: `false`) |
| `owner` | No | チームブログのオーナー名 |

### 設定の優先順位

ブログ固有設定 > `default` セクション > 環境変数

ローカル設定ファイルとグローバル設定ファイルが両方ある場合、ローカルが優先されつつグローバルで補完される。

## エントリ形式

blogsync と同じ YAML frontmatter + Markdown:

```markdown
---
Title: 記事タイトル
Category:
- tech
- rust
Date: 2025-01-01T00:00:00+09:00
URL: https://blog.example.com/entry/2025/01/01/000000
EditURL: https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/123456
Draft: true
CustomPath: my-custom-path
---

記事の本文がここに入る。
```

## ライセンス

MIT
