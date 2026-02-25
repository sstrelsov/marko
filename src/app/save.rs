//! File saving: write editor content to disk and docx export.

use super::*;

impl<'a> App<'a> {
    /// Writes the current editor content to disk and resets the modified flag.
    pub(super) fn save(&mut self) {
        let save_content = self.textarea_content();
        match std::fs::write(&self.file_path, &save_content) {
            Ok(_) => {
                self.original_content = save_content;
                self.modified = false;

                // Round-trip: also export back to .docx if we're in docx mode
                if let Some(ref ds) = self.docx_state {
                    match pandoc::md_to_docx(&self.file_path, &ds.docx_path, Some(&ds.reference_doc)) {
                        Ok(_) => self.set_status("Saved (.md + .docx)"),
                        Err(e) => self.set_status(&format!("Saved .md, but .docx failed: {}", e)),
                    }
                } else {
                    self.set_status("Saved");
                }

                self.refresh_git_status();
                self.refresh_gutter_marks();
            }
            Err(e) => {
                self.set_status(&format!("Error saving: {}", e));
            }
        }
    }
}
