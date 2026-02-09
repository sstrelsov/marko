# Plan: Make `marko` installable and self-updatable via GitHub

## Context

Marko is a fully working terminal markdown editor, but it can only be run via `cargo run` from the project directory. The goal is to make it a proper CLI tool: `marko <file>` works from anywhere, and `marko upgrade` pulls the latest version from GitHub.

## Step 1: Add `Upgrade` subcommand to existing CLI

**File:** `src/main.rs`

The CLI already has a `Commands` enum (with `Export`). Add an `Upgrade` variant:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Export markdown to .docx
    Export { /* ...existing fields... */ },
    /// Update marko to the latest version
    Upgrade,
}
```

Dispatch in `main()`: match `Some(Commands::Upgrade)` alongside the existing `Export` arm. Also switch `#[command(version = "0.1.0", ...)]` to `#[command(version, ...)]` so the version is read from `Cargo.toml` automatically (single source of truth).

## Step 2: Add self-update via `self_update` crate

**New file:** `src/upgrade.rs`
**Modify:** `Cargo.toml` (add `self_update` dep), `src/lib.rs` (add `pub mod upgrade`)

The `self_update` crate's GitHub backend will:

1. Check the latest GitHub Release on the repo
2. Find the right binary for the current platform (by target triple in the asset name)
3. Download, extract, and replace the running binary in-place

This gives `marko upgrade` → downloads latest release binary → done.

## Step 3: Create `.gitignore`, init git, push to GitHub

- Create a `.gitignore` (`/target`, `.DS_Store`, `*.thumb.png`, etc.)
- `git init`, commit, create a GitHub repo via `gh repo create`, push

## Step 4: Add GitHub Actions release workflow

**New file:** `.github/workflows/release.yml`

On tag push (`v*`), builds release binaries for:

- `aarch64-apple-darwin` (Apple Silicon)
- `x86_64-apple-darwin` (Intel Mac)
- `x86_64-unknown-linux-gnu` (Linux)

Creates a GitHub Release with `.tar.gz` archives attached. Asset naming follows the pattern `self_update` expects: `marko-v0.1.0-aarch64-apple-darwin.tar.gz`.

## Step 5: Install locally and test

- `cargo install --path .` → puts `marko` in `~/.cargo/bin/` (already in PATH from rustup)
- Verify: `marko somefile.md` opens the editor, `marko upgrade` checks for updates

## Files to create/modify

| File | Action |
|------|--------|
| `src/main.rs` | Modify — add `Upgrade` variant to `Commands` enum + dispatch |
| `src/upgrade.rs` | Create — self-update logic |
| `src/lib.rs` | Modify — add `pub mod upgrade` |
| `Cargo.toml` | Modify — add `self_update` dependency |
| `.gitignore` | Create |
| `.github/workflows/release.yml` | Create |

## Verification

1. `cargo build` — compiles without errors
2. `cargo run -- somefile.md` — opens editor (existing behavior preserved)
3. `cargo run -- upgrade` — prints "checking for updates" (will fail gracefully until GitHub releases exist)
4. `cargo run -- export somefile.md` — existing export still works
5. `cargo run -- --version` — prints version from Cargo.toml
6. `cargo run` (no args) — prints usage/help
7. `cargo test` — existing tests still pass
