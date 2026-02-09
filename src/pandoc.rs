use std::fmt;
use std::io;
use std::path::Path;
use std::process::Command;

/// Errors that can occur when invoking pandoc.
#[derive(Debug)]
pub enum PandocError {
    /// Pandoc is not installed or not found on PATH.
    NotInstalled,
    /// Pandoc ran but exited with a non-zero status.
    ConversionFailed { stderr: String, exit_code: i32 },
    /// An I/O error occurred while spawning the process.
    Io(io::Error),
}

impl fmt::Display for PandocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PandocError::NotInstalled => write!(f, "pandoc is not installed"),
            PandocError::ConversionFailed { stderr, exit_code } => {
                write!(f, "pandoc exited with code {}: {}", exit_code, stderr)
            }
            PandocError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<io::Error> for PandocError {
    fn from(e: io::Error) -> Self {
        PandocError::Io(e)
    }
}

/// Returns `true` if pandoc is installed and runnable.
pub fn is_available() -> bool {
    Command::new("pandoc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Converts a markdown file to .docx via pandoc.
///
/// If `reference_doc` is provided, it is passed as `--reference-doc` so that
/// the output inherits the styling from the reference document.
pub fn md_to_docx(
    md_path: &Path,
    docx_path: &Path,
    reference_doc: Option<&Path>,
) -> Result<(), PandocError> {
    let mut cmd = Command::new("pandoc");
    cmd.arg(md_path)
        .arg("-o")
        .arg(docx_path)
        .arg("--from=markdown")
        .arg("--to=docx");

    if let Some(ref_doc) = reference_doc {
        cmd.arg(format!("--reference-doc={}", ref_doc.display()));
    }

    let output = cmd.output().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PandocError::NotInstalled
        } else {
            PandocError::Io(e)
        }
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(PandocError::ConversionFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

/// Converts a .docx file to GitHub-Flavored Markdown via pandoc.
///
/// Returns the markdown content as a string.
pub fn docx_to_md(docx_path: &Path) -> Result<String, PandocError> {
    let output = Command::new("pandoc")
        .arg(docx_path)
        .arg("--from=docx")
        .arg("--to=gfm")
        .arg("--wrap=none")
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                PandocError::NotInstalled
            } else {
                PandocError::Io(e)
            }
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(PandocError::ConversionFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn is_available_does_not_panic() {
        // Just ensure it returns a bool without panicking
        let _ = is_available();
    }

    #[test]
    fn md_to_docx_basic_conversion() {
        if !is_available() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let md_path = dir.path().join("test.md");
        let docx_path = dir.path().join("test.docx");
        fs::write(&md_path, "# Hello\n\nWorld").unwrap();

        let result = md_to_docx(&md_path, &docx_path, None);
        assert!(result.is_ok(), "md_to_docx failed: {:?}", result.err());
        assert!(docx_path.exists(), ".docx file should be created");
        assert!(
            fs::metadata(&docx_path).unwrap().len() > 0,
            ".docx file should not be empty"
        );
    }

    #[test]
    fn docx_to_md_round_trip() {
        if !is_available() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let md_path = dir.path().join("test.md");
        let docx_path = dir.path().join("test.docx");
        fs::write(&md_path, "# Hello\n\nThis is a test paragraph.").unwrap();

        md_to_docx(&md_path, &docx_path, None).unwrap();
        let markdown = docx_to_md(&docx_path).unwrap();
        assert!(
            markdown.contains("Hello"),
            "Round-tripped markdown should contain 'Hello', got: {}",
            markdown
        );
        assert!(
            markdown.contains("test paragraph"),
            "Round-tripped markdown should contain 'test paragraph', got: {}",
            markdown
        );
    }

    #[test]
    fn md_to_docx_with_reference_doc() {
        if !is_available() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let md_path = dir.path().join("test.md");
        let docx_path = dir.path().join("output.docx");
        let ref_path = dir.path().join("reference.docx");
        fs::write(&md_path, "# Styled\n\nContent here").unwrap();

        // Create a reference doc first
        md_to_docx(&md_path, &ref_path, None).unwrap();

        // Now convert with reference doc
        let result = md_to_docx(&md_path, &docx_path, Some(&ref_path));
        assert!(
            result.is_ok(),
            "md_to_docx with reference_doc failed: {:?}",
            result.err()
        );
        assert!(docx_path.exists());
    }

    #[test]
    fn md_to_docx_nonexistent_input() {
        if !is_available() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let md_path = dir.path().join("nonexistent.md");
        let docx_path = dir.path().join("out.docx");

        let result = md_to_docx(&md_path, &docx_path, None);
        assert!(result.is_err(), "Should fail on nonexistent input");
    }
}
