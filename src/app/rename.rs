//! Inline file rename mode: enter, edit, and confirm/cancel file renames.
//!
//! Activated via Ctrl+T or clicking the filename in the header bar.
//! Handles both plain .md files and .docx round-trip pairs.

use super::*;

impl<'a> App<'a> {
    // ─── Rename mode ─────────────────────────────────────────────────────

    /// Enter rename mode: populates the rename buffer with the current filename
    /// and places the cursor at the end.
    pub(super) fn start_rename(&mut self) {
        let source_path = if let Some(ref ds) = self.docx_state {
            &ds.docx_path
        } else {
            &self.file_path
        };
        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        self.rename_buf = filename;
        self.rename_cursor = self.rename_buf.len();
        self.renaming = true;
    }

    /// Handles keypresses while in rename mode.
    /// Enter confirms, Esc cancels, printable chars edit the name.
    pub(super) fn handle_rename_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.renaming = false;
                self.rename_buf.clear();
            }
            KeyCode::Enter => {
                self.confirm_rename();
            }
            KeyCode::Backspace => {
                if self.rename_cursor > 0 {
                    self.rename_cursor -= 1;
                    self.rename_buf.remove(self.rename_cursor);
                }
            }
            KeyCode::Delete => {
                if self.rename_cursor < self.rename_buf.len() {
                    self.rename_buf.remove(self.rename_cursor);
                }
            }
            KeyCode::Left => {
                if self.rename_cursor > 0 {
                    self.rename_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.rename_cursor < self.rename_buf.len() {
                    self.rename_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.rename_cursor = 0;
            }
            KeyCode::End => {
                self.rename_cursor = self.rename_buf.len();
            }
            KeyCode::Char(ch) => {
                // Reject path separators to keep the name a bare filename
                if ch != '/' && ch != '\\' {
                    self.rename_buf.insert(self.rename_cursor, ch);
                    self.rename_cursor += 1;
                }
            }
            _ => {}
        }
    }

    /// Performs the actual file rename via fs::rename, updates internal state.
    /// When in docx mode, renames both the .docx and .md files.
    fn confirm_rename(&mut self) {
        let new_name = self.rename_buf.trim().to_string();
        if new_name.is_empty() {
            self.set_status("Rename cancelled: empty name");
            self.renaming = false;
            return;
        }

        if let Some(ref ds) = self.docx_state {
            // Docx mode: the user is renaming the .docx file
            let current_name = ds
                .docx_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if new_name == current_name {
                self.renaming = false;
                return;
            }

            let new_docx_path = ds.docx_path.with_file_name(&new_name);
            // Derive the .md sibling name from the new .docx name
            let new_md_path = new_docx_path.with_extension("md");

            // Rename the .docx file
            match std::fs::rename(&ds.docx_path, &new_docx_path) {
                Ok(_) => {
                    // Rename the .md file too
                    let md_renamed = std::fs::rename(&self.file_path, &new_md_path);
                    self.file_path = new_md_path;
                    self.docx_state = Some(DocxState {
                        docx_path: new_docx_path.clone(),
                        reference_doc: new_docx_path,
                    });
                    if md_renamed.is_ok() {
                        self.set_status("Renamed");
                    } else {
                        self.set_status("Renamed .docx (but .md rename failed)");
                    }
                    self.refresh_git_status();
                    self.refresh_gutter_marks();
                }
                Err(e) => {
                    self.set_status(&format!("Rename failed: {}", e));
                }
            }
        } else {
            // Regular .md mode
            let current_name = self
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if new_name == current_name {
                self.renaming = false;
                return;
            }

            let new_path = self.file_path.with_file_name(&new_name);
            match std::fs::rename(&self.file_path, &new_path) {
                Ok(_) => {
                    self.file_path = new_path;
                    self.set_status("Renamed");
                    self.refresh_git_status();
                    self.refresh_gutter_marks();
                }
                Err(e) => {
                    self.set_status(&format!("Rename failed: {}", e));
                }
            }
        }
        self.renaming = false;
    }
}
