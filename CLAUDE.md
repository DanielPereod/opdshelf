# OPDShelf

Self-hosted OPDS library server in Rust, with a web panel, an Android app and a KOReader plugin.

## Layout
- `src/` Rust server (axum). `src/main.rs` starts it, `src/config.rs` reads the env vars.
- `views/` Handlebars templates; `static/` CSS, JS and readers (built with `npm run build`).
- `tests/` Rust integration tests.
- `android/` Kotlin app (strings in `res/values` and `res/values-es`).
- `koreader/opdshelf.koplugin` KOReader plugin in Lua; its tests in `koreader/test`.
- `docs/` user docs.

## Principles
- One binary, no database: all state lives as files in `DATA_DIR` (default `BOOKS_DIR/.opdshelf`).
- Never modify the book files. Edits, covers and progress go to `DATA_DIR`; `BOOKS_DIR` may be read-only.
- Must work on NAS and network shares (SMB/NFS, where `WATCH=false` falls back to periodic scans).
- Every user-facing text in English and Spanish: web texts are written in English and translated in `src/i18n.rs`.

## Checks (all must pass, as in CI)
```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
for t in plugin library browser; do luajit koreader/test/${t}_test.lua koreader/opdshelf.koplugin; done
(cd android && ./gradlew testDebugUnitTest assembleRelease)
```

## Branches
`main` is stable, `beta` is development. Open PRs against `beta`.

## Git authorship
- Commits by `DanielPereod <45797916+DanielPereod@users.noreply.github.com>` (author and committer).
- No mention of Claude in commits or PRs: no `Co-Authored-By`, session links or "Generated with" lines.
