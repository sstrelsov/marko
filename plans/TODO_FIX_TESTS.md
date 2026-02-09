# Fix Flaky E2E Tests

Three tests in `tests/e2e_test.rs` fail intermittently (or consistently on some machines). All share a common root cause: fixed `short_delay()` sleeps (200ms) are insufficient for the TUI to fully process input before the next action.

---

## 1. `backspace_0x7f_deletes_one_character` (line 131)

**Symptom:** File still contains `"abcde"` — the backspace had no effect.

**Root cause:** The `End` key (`\x1b[F`) moves the cursor to EOL, but the 200ms `short_delay()` isn't always enough for the TUI to process it before the backspace arrives. When the backspace fires while the cursor is still at position 0, it's a no-op.

**Fix:** Replace `short_delay()` after sending `End` with an `expect()` that confirms the cursor actually moved (e.g. wait for a re-render showing the cursor column in the status bar), or increase the delay. Alternatively, send `End` + `Backspace` in a single `send()` call and let crossterm's event queue serialize them, then add a render-gate before saving.

---

## 2. `app_type_and_save_persists_to_disk` (line 72)

**Symptom:** `expect("Saved")` times out — the status message never appears.

**Root cause:** The 200ms delay after typing `"ADDED"` may not be enough for all 5 keystrokes to be processed. If `Ctrl+S` arrives before the editor has processed the text, the save may race. More likely, the `expect("Saved")` timeout (5s default) is borderline when the system is under load (e.g. parallel test execution).

**Fix:** Add an `expect()` gate after typing that confirms the text appeared in the editor before sending `Ctrl+S`. The status bar shows column position — waiting for `Col 5` (or similar) would confirm input was processed.

---

## 3. `rename_flow_confirm` (line 246)

**Symptom:** `renamed.md` doesn't exist on disk after the test.

**Root cause:** The test sends `Home` then 7 individual `Delete` keypresses with only 20ms between each. If any delete is dropped or arrives out of order, the rename buffer won't be fully cleared, so the final name becomes something like `terenamed.md` instead of `renamed.md`. The `Enter` confirms a wrong name, and `renamed.md` is never created.

**Fix:** Instead of 7 individual deletes at 20ms intervals, either:
- Select-all in the rename buffer (if supported) before typing the new name
- Use a longer per-keystroke delay (50-100ms)
- Send `Ctrl+A` or `Home` + `Shift+End` + `Delete` to clear in one shot
- Add an `expect()` gate after clearing that confirms the buffer is empty before typing the new name

---

## General Recommendations

1. **Replace blind `short_delay()` with render-gated expects.** Instead of sleeping a fixed duration and hoping the TUI caught up, `expect()` a known string that proves the previous action took effect. This makes tests deterministic regardless of system load.

2. **Increase `send_and_wait` delay as a stopgap.** Bumping from 200ms to 400-500ms would reduce flakiness but not eliminate it. Prefer render-gating.

3. **Consider a test-mode status bar** that always shows cursor position + mode, giving tests a reliable anchor to `expect()` on after every action.
