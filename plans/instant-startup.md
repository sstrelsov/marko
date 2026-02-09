# Instant Startup: Defer Expensive Initialization to Background Threads

## Context

`App::new()` blocks for ~3 seconds before the first render because it eagerly performs:

1. **syntect loading (~1.5-2s)**: `SyntaxSet::load_defaults_newlines()` parses hundreds of bundled syntax definitions
2. **Git gutter marks (~0.5-1s)**: Full repo diff with rename detection via `compute_gutter_marks()`
3. **Code fence highlighting (~0.1-0.3s)**: Pre-computes all highlights

The user sees a blank terminal for 3 seconds. All three operations produce *cosmetic overlays* — the editor is fully functional without them.

## Approach

Spawn background threads for expensive work and render the editor immediately. Syntax highlighting and gutter marks appear progressively within 1-2s.

---

## Changes

### 1. `src/markdown/code_highlight.rs` — Non-blocking syntect loading

Switch `LazyLock` → `OnceLock` to support a non-blocking `is_ready()` check:

- Replace `static SYNTAX_SET: LazyLock<SyntaxSet>` and `THEME_SET` with `OnceLock`
- Update `syntax_set()` / `theme_set()` to use `get_or_init()` (still blocking for callers that need the value)
- Update `highlight_code()` internal references from `&*SYNTAX_SET` to `syntax_set()`
- Add `pub fn ensure_loaded()` — spawns a thread to warm up both statics
- Add `pub fn try_get() -> Option<(&'static SyntaxSet, &'static ThemeSet)>` — non-blocking check

### 2. `src/main.rs` — Pre-warm syntect at program start

Add one line at the top of `main()` before `Cli::parse()`:

```rust
marko::markdown::code_highlight::ensure_loaded();
```

This gives the background thread a ~1s head start while git operations run.

### 3. `src/app.rs` — Defer initialization, poll for results

**Struct changes:**

- Remove `syntax_set: &'static SyntaxSet` and `theme_set: &'static ThemeSet` fields (lines 112-113)
- Remove `use syntect::highlighting::ThemeSet` and `use syntect::parsing::SyntaxSet` imports (lines 18-19)
- Add `gutter_handle: Option<std::thread::JoinHandle<HashMap<usize, GutterMark>>>` field
- Add `use std::thread::JoinHandle` import

**`App::new()` (lines 192-257):**

- Keep git repo open + branch name + file status (fast, needed for header)
- Replace synchronous `compute_gutter_marks()` with `std::thread::spawn` that reopens the repo via `git2::Repository::discover()` and computes gutter marks. Store `JoinHandle` in new field. Init `gutter_marks: HashMap::new()`
- Remove `syntax_set()`/`theme_set()` calls and `highlight_code_regions()` call
- Init `code_fence_highlights: vec![]` and `code_fence_dirty: true`

**`tick()` (line 631):**

- Add polling: check `gutter_handle.is_finished()`, if done, `.take()` + `.join()` to populate `gutter_marks`

**`apply_code_fence_highlighting()` (line 545):**

- When `code_fence_dirty`, check `code_highlight::try_get()` first
- If `None` (syntect still loading), return early — leave `dirty=true` for retry next frame
- If `Some((ss, ts))`, proceed with existing recompute logic using those references

**`highlight_code_regions()` (line 134):**

- No signature change — still takes `&SyntaxSet` and `&ThemeSet` as params

**`refresh_gutter_marks()` (line 1460):**

- Add `self.gutter_handle = None;` at top to discard any pending background computation

**Tests:**

- Delete `app_uses_shared_syntax_set_reference` and `app_uses_shared_theme_set_reference` tests (lines 2017-2027) — fields no longer exist

---

## Verification

1. `cargo build` — compiles without errors
2. `cargo test` — all tests pass (two removed tests were the only ones touching deleted fields)
3. `cargo run -- EXAMPLE.md` — editor appears instantly, highlights + gutter marks appear within 1-2s
4. Edit a code fence → highlights update on keystroke (dirty mechanism still works)
5. Save file → gutter marks refresh (refresh_gutter_marks still works synchronously on save)
