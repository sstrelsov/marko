use std::collections::HashMap;
use std::path::Path;

use git2::{DiffFindOptions, DiffOptions, Patch, Repository};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GutterMark {
    Added,    // New lines not in HEAD (green)
    Modified, // Lines that replaced other lines (yellow)
    Removed,  // Deletion point indicator (red)
}

/// Returns a map of 0-indexed line numbers → gutter marks for the current file.
pub fn compute_gutter_marks(repo: &Repository, file_path: &Path) -> HashMap<usize, GutterMark> {
    let workdir = match repo.workdir() {
        Some(w) => w,
        None => return HashMap::new(),
    };

    let relative = match file_path.canonicalize() {
        Ok(canon) => {
            let canon_workdir = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
            match canon.strip_prefix(&canon_workdir) {
                Ok(r) => r.to_path_buf(),
                Err(_) => return HashMap::new(),
            }
        }
        Err(_) => match file_path.strip_prefix(workdir) {
            Ok(r) => r.to_path_buf(),
            Err(_) => return HashMap::new(),
        },
    };

    // Don't set pathspec — rename detection needs full diff to match old→new
    let mut diff_opts = DiffOptions::new();

    let head_tree = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());

    let mut diff = match repo.diff_tree_to_workdir(head_tree.as_ref(), Some(&mut diff_opts)) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };

    // Enable rename detection so renamed files diff against their old content
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    let _ = diff.find_similar(Some(&mut find_opts));

    let num_deltas = diff.deltas().len();
    let mut marks = HashMap::new();

    for delta_idx in 0..num_deltas {
        // Only process deltas that touch our file
        let delta = diff.deltas().nth(delta_idx).unwrap();
        if delta.new_file().path().map(|p| p.to_path_buf()).as_deref() != Some(relative.as_path())
        {
            continue;
        }

        let patch = match Patch::from_diff(&diff, delta_idx) {
            Ok(Some(p)) => p,
            _ => continue,
        };

        let num_hunks = patch.num_hunks();
        for hunk_idx in 0..num_hunks {
            let (_, num_lines) = patch.hunk(hunk_idx).unwrap();
            let mut added_lines = Vec::new();
            let mut has_deletions = false;
            let mut deletion_point: Option<usize> = None;

            for line_idx in 0..num_lines {
                if let Ok(line) = patch.line_in_hunk(hunk_idx, line_idx) {
                    match line.origin() {
                        '+' => {
                            if let Some(new_lineno) = line.new_lineno() {
                                added_lines.push((new_lineno as usize) - 1); // 0-indexed
                            }
                        }
                        '-' => {
                            has_deletions = true;
                            // The deletion point is the new-file line where removals happen.
                            // For context after a deletion, new_lineno gives us the right spot.
                            if deletion_point.is_none() {
                                // Use the old_lineno mapped to the new file position.
                                // The next context or addition line's new_lineno - 1 gives us the
                                // deletion point, but we can also compute it from the hunk header.
                                let (hunk_header, _) = patch.hunk(hunk_idx).unwrap();
                                let new_start = hunk_header.new_start() as usize;
                                // Deletion happened before the new_start line (0-indexed)
                                deletion_point = Some(new_start.saturating_sub(1));
                            }
                        }
                        _ => {}
                    }
                }
            }

            if !added_lines.is_empty() {
                let mark = if has_deletions {
                    GutterMark::Modified
                } else {
                    GutterMark::Added
                };
                for line in added_lines {
                    marks.insert(line, mark);
                }
            } else if has_deletions {
                // Pure deletion: mark the deletion point
                if let Some(point) = deletion_point {
                    marks.insert(point, GutterMark::Removed);
                }
            }
        }
    }

    marks
}
