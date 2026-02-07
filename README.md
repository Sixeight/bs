# bs

[blogsync](https://github.com/x-motemen/blogsync) 互換のはてなブログ管理CLIツール。Rust製。

## インストール

```sh
cargo install --path .
```

## 設定

blogsync と同じ形式の YAML 設定ファイルを使う。

**検索順序:**
1. `./blogsync.yaml`（カレントディレクトリ）
2. `~/.config/blogsync/config.yaml`

```yaml
default:
  local_root: ~/blog

example.hatenablog.com:
  username: your-hatena-id
  password: your-api-key
```

環境変数でも設定できる:

| 環境変数 | 説明 |
|---|---|
| `BLOGSYNC_USERNAME` | はてなID |
| `BLOGSYNC_PASSWORD` | APIキー |
| `BLOGSYNC_WORKDIR` | ローカルルート / 作業ディレクトリ (`-C` と同等) |

## 使い方

### 記事の一括ダウンロード

```sh
bs pull example.hatenablog.com
```

設定済みの全ブログをpull:

```sh
bs pull
```

下書きのみ/下書き除外:

```sh
bs pull --only-drafts
bs pull --no-drafts
```

### 記事の個別取得

```sh
bs fetch path/to/entry.md
```

### 記事の投稿（標準入力から）

```sh
echo "本文" | bs post example.hatenablog.com --title "タイトル"
echo "下書き" | bs post example.hatenablog.com --title "WIP" --draft
echo "固定ページ" | bs post example.hatenablog.com --title "About" --page
echo "カスタムURL" | bs post example.hatenablog.com --custom-path "my-entry"
```

### 記事の更新

ローカルのMarkdownファイルを編集してpush:

```sh
bs push path/to/entry.md
```

下書きを公開:

```sh
bs push path/to/entry.md --publish
```

### 記事の削除

```sh
bs remove path/to/entry.md
```

### 設定済みブログの一覧

```sh
bs list
```

### コマンドエイリアス

| コマンド | エイリアス |
|---|---|
| `pull` | `pl` |
| `list` | `ls` |
| `remove` | `rm` |

## 記事のファイル形式

blogsync と同じ YAML frontmatter + Markdown 形式:

```markdown
---
Title: 記事タイトル
Category:
- tech
- rust
Date: 2025-01-01T00:00:00+09:00
URL: https://example.hatenablog.com/entry/2025/01/01/000000
EditURL: https://blog.hatena.ne.jp/user/example.hatenablog.com/atom/entry/123456
Draft: false
---

記事の本文がここに入る。
```

## ベンチマーク

blogsync v0.20.1 との比較（[hyperfine](https://github.com/sharkdp/hyperfine) で計測、Apple Silicon）:

| コマンド | bs (Rust) | blogsync (Go) | 比率 |
|---|---|---|---|
| `--help`（起動速度） | **1.9 ms** | 5.8 ms | **3.0x 高速** |
| `list`（ローカル操作） | **7.2 ms** | 7.4 ms | 同等 |
| `pull`（2489記事取得） | **26.0 s** | 27.2 s | 1.04x 高速 |
| User CPU（pull時） | **0.09 s** | 0.32 s | **3.5x 軽量** |
| バイナリサイズ | **3.4 MB** | 12 MB | **3.5x 小さい** |

> `pull` はネットワーク I/O が支配的なため実行時間の差は小さいが、CPU 使用量は約 3.5 倍効率的。起動速度は 3 倍速い。

## blogsync との互換性

- 設定ファイル形式が完全互換
- 記事のファイル形式が完全互換
- コマンド体系が完全互換
- 既存の blogsync 環境にそのまま導入可能

## ライセンス

MIT
