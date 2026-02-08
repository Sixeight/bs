# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                  # dev build
cargo build --release        # optimized (LTO, strip, panic=abort)
cargo test                   # run all 71 tests
cargo test <pattern>         # run matching tests, e.g. cargo test config::tests::test_resolve
cargo test -- --list         # list all tests without running
cargo clippy                 # lint
cargo fmt -- --check         # format check
```

## Architecture

**bs** is a Rust CLI tool compatible with [blogsync](https://github.com/x-motemen/blogsync) for managing Hatena Blog via AtomPub API.

### Module Layout

- `src/main.rs` — CLI entry point (clap 4 derive). Defines subcommands: pull, push, post, fetch, list, remove.
- `src/progress.rs` — Terminal spinner (TTY) / structured log (non-TTY) output.
- `src/client/mod.rs` — `HatenaClient`: wraps reqwest with a **single-thread tokio runtime** (`new_current_thread`). Each client owns its runtime; async ops are driven via `block_on()`.
- `src/client/atom.rs` — AtomPub XML parsing/building with quick-xml. `Entry` and `Feed` structs.
- `src/client/wsse.rs` — WSSE authentication: SHA-1 nonce + base64 for `X-WSSE` header.
- `src/config/mod.rs` — YAML config loading. Merges local (`./blogsync.yaml`) → global (`~/.config/blogsync/config.yaml`) → env vars (`BLOGSYNC_USERNAME`, `BLOGSYNC_PASSWORD`). `detect_blog_from_path()` finds the matching blog by longest path prefix.
- `src/entry/mod.rs` — `LocalEntry`: YAML frontmatter + Markdown body. Handles file path resolution, draft storage (`_draft/{id}.md`), mtime/git-date freshness, custom YAML quoting (Go-compatible).
- `src/commands/` — One file per subcommand. Pull uses async prefetch via `spawn_fetch()` + mpsc channel. Push supports both PUT (existing) and POST (new entry without EditURL).

### Key Design Decisions

- **Single-thread tokio per client** — no multi-thread overhead; HTTP/2 multiplexing handles concurrency.
- **Freshness by mtime** — pull skips if remote ≤ local; push skips if local ≤ remote. Git author date preferred over file mtime. Shallow repos are auto-unshallowed.
- **Draft path heuristic** — auto-generated URL patterns (`entry/YYYY/MM/DD/anything`, `entry/YYYYMMDD/timestamp`) get stored under `_draft/` to avoid path conflicts. The `YYYY/MM/DD/` pattern matches any trailing segment (digits or title strings), matching Go's `titlePathReg`.
- **Go-compatible YAML quoting** — custom serialization matches blogsync's Go YAML marshaler output for roundtrip compatibility.
- **XML building matches Go** — `<app:control>` with `<app:draft>` and `<app:preview>` only emitted for drafts. `<content>` has no type attribute. `<updated>` emitted when non-empty.
- **Push without EditURL** — files without EditURL are treated as new entries; blog is detected from file path via `detect_blog_from_path()`, entry path becomes CustomPath if not auto-generated.
- **Draft date clearing** — `from_atom()` clears past dates on draft entries to avoid sending stale timestamps on push.

### Dependencies

- `serde_yml` (not serde_yaml), `time` crate (not chrono), `getrandom` (not rand)
- `reqwest` 0.12 with `rustls-tls`, `http2`, `gzip` features (no default features)
- `quick-xml` 0.37 with `serialize` feature

## Entry File Format

```markdown
---
Title: "Article Title"
Category:
- rust
Date: 2025-01-01T00:00:00+09:00
URL: https://example.com/entry/2025/01/01/123456
EditURL: https://blog.hatena.ne.jp/user/example.com/atom/entry/456789
Draft: false
---

Body in Markdown.
```

Draft field accepts `true`/`false` and `"yes"`/`"no"` (case-insensitive) via custom deserializer.

## Config Format

```yaml
default:
  local_root: ~/blog
blog.example.com:
  username: hatena-id
  password: api-key
```

Config merging: local values take priority, global fills gaps. `omit_domain: true` strips blog domain from file paths.

## Testing

All tests are unit tests within each module (no integration test directory). Key areas:
- `entry::tests` (32): frontmatter roundtrip, path resolution, draft handling, YAML quoting, date clearing
- `config::tests` (18): parsing, merging, env vars, tilde expansion, detect_blog_from_path
- `client::atom::tests` (12): XML parse/build, special character escaping, draft-only control
- `commands::fetch::tests` (4): blog domain extraction
- `client::wsse::tests` (3): header format validation
- `commands::push::tests` (2): entry path extraction, draft dir detection
