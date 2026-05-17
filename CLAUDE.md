# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language

Communicate with the user in **Chinese**.

## Development Commands

```bash
# Lint (must pass with no warnings)
cargo clippy --workspace -- -D warnings

# Format check (do not auto-format external submodules)
cargo fmt --check --all

# Run all tests
cargo test --workspace

# Run updater tests with output
cargo test -p custom-utils --features updater -- --nocapture
```

All commands run from the workspace root. **After every code change, run all three in order and fix any failures before considering the task done.** Never stop with "please test this yourself."

### cargo-make tasks (requires `cargo install cargo-make`)

```bash
cargo make dev_win    # --features=dev,logger,daemon (Windows)
cargo make dev_unix   # --features=dev,logger,daemon (Linux)
cargo make prod       # --features=prod,logger
```

## Architecture

This is a **Rust 2021 workspace** with two members:
- `custom-utils` (root) — feature-gated utility library, v0.11.x
- `examples/logger` — standalone example crate, depends on the root crate via path

### Feature-gated modules

| Feature | Module | Notes |
|---------|--------|-------|
| `logger` (default) | `util_logger` | `flexi_logger`-based; file rotation, JSON, colored output |
| *(always on)* | `util_args` | Lightweight CLI arg parsing; no external parser |
| `daemon-async` / `daemon-sync` | `util_daemon` | Systemd watchdog; real integration only under `prod` + Linux |
| `tls` | `util_tls` | Rustls cert/key loading from disk |
| `tls-util` | `util_tls_util` | X.509 cert generation via `picky`; no OpenSSL |
| `derive` | `util_derive` | Async file parsing via `syn` |
| `updater` | `util_updater` | GitHub Release auto-updater (async, streams to disk) |
| `timer` | `util_timer` | Delegates to external `timer-util` crate |

The daemon module uses platform-conditional compilation: on non-Linux or without the `prod` feature it compiles to a no-op.

### Workspace dependencies

All member crates reference shared dependencies with `{ workspace = true }`. Versions are declared once in the root `[workspace.dependencies]`. Never duplicate version strings across crate manifests.

## Code Quality Rules

**Error handling:**
- Library code (`lib.rs`, submodules): use `anyhow::Result` + `?` + `.context("…")`. No `.unwrap()` / `.expect()`.
- `main.rs` and test code: `.unwrap()` is allowed.
- Never suppress warnings with `#[allow(…)]` without a comment explaining why.

**Safety and performance:**
- No `unsafe` blocks without a detailed comment justifying necessity and safety guarantees.
- Prefer borrowing (`&T`) over `.clone()`; avoid gratuitous clones.
- In `async` contexts, never call blocking APIs (`std::fs`, `std::thread::sleep`, synchronous network I/O). Use `tokio` equivalents or `tokio::task::spawn_blocking`.

**Visibility:** Default to private. Use `pub(crate)` before `pub`; expose only what external callers genuinely need.

**HTTP:** Use `reqwest` with `default-features = false` and explicit features, e.g.:
```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }
```
Always use the async `Client`; never the blocking API.

**Logging:** Use `log` macros (`info!`, `error!`, etc.). Initialize with `custom_utils::logger::logger_feature`. No `println!` for application logs.

## CI / Release

Build targets: `x86_64-pc-windows-msvc`, `aarch64-unknown-linux-gnu`. Pushing a `v*` tag triggers the GitHub Release workflow. Confirm the fix loop passes locally before pushing a release tag.
