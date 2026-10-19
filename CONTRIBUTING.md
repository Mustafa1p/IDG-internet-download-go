# Contributing to IDG

Thank you for considering a contribution! This document explains how to get
set up, the code conventions we follow, and the pull request workflow.

---

## Getting started

```powershell
# 1. Fork the repo on GitHub, then clone your fork
git clone https://github.com/<YOUR_USERNAME>/IDG.git
cd IDG

# 2. Create a feature branch
git checkout -b feat/my-feature

# 3. Build and run
cargo run
```

---

## Code conventions

| Rule | Detail |
|---|---|
| **Formatting** | `cargo fmt` — no exceptions |
| **Lints** | `cargo clippy -- -D warnings` must pass with zero errors |
| **Doc comments** | Every `pub` function and type must have a `///` doc comment |
| **Error handling** | Use `?` with `anyhow::Context`; no `.unwrap()` in non-test code |
| **Naming** | Follow standard Rust naming: `snake_case` functions, `PascalCase` types |
| **No unsafe** | Unsafe blocks require a `// SAFETY:` comment explaining the invariants |

---

## Module overview

| Module | Responsibility |
|---|---|
| `src/main.rs` | Runtime setup and eframe bootstrap only — keep it thin |
| `src/app.rs` | GUI rendering — no business logic; only read/write `DownloadItem` |
| `src/download/types.rs` | Shared data types — avoid adding methods with side effects here |
| `src/download/probe.rs` | HTTP HEAD probe — pure async function, no global state |
| `src/download/segment.rs` | Single range-request worker — no GUI code |
| `src/download/engine.rs` | Coordinates probe + workers — the only place that calls `with_item` in bulk |

---

## Pull request checklist

- [ ] `cargo fmt` applied
- [ ] `cargo clippy -- -D warnings` passes
- [ ] New `pub` items have `///` doc comments
- [ ] No `.unwrap()` in non-test code
- [ ] CHANGELOG.md updated under `[Unreleased]`
- [ ] PR description explains *what* and *why*

---

## Reporting bugs

Open an [issue](https://github.com/mustafa1p/IDG/issues/new) with:
- IDG version (from the status bar)
- Windows version
- URL that failed (if safe to share)
- Steps to reproduce
- Expected vs actual behaviour

---

## Feature requests

Open an [issue](https://github.com/mustafa1p/IDG/issues/new) tagged `enhancement`.
Describe the use case, not just the feature, so we can find the best solution.
