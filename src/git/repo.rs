use git2::{Repository, StatusOptions};
use std::path::Path;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(file_path: &Path) -> Option<Self> {
        let dir = file_path.parent()?;
        Repository::discover(dir).ok().map(|repo| Self { repo })
    }

    pub fn branch_name(&self) -> String {
        self.repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(String::from))
            .unwrap_or_else(|| "HEAD".to_string())
    }

    pub fn file_status(&self, file_path: &Path) -> String {
        let workdir = match self.repo.workdir() {
            Some(w) => w,
            None => return String::new(),
        };

        // Try direct prefix strip first, then canonicalized paths
        if let Ok(relative) = file_path.strip_prefix(workdir) {
            return self.status_string(relative);
        }

        let canon_file = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
        let canon_workdir = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
        match canon_file.strip_prefix(&canon_workdir) {
            Ok(relative) => self.status_string(relative),
            Err(_) => String::new(),
        }
    }

    fn status_string(&self, relative: &Path) -> String {
        let mut opts = StatusOptions::new();
        opts.pathspec(relative.to_string_lossy().as_ref());
        opts.include_untracked(true);

        let statuses = match self.repo.statuses(Some(&mut opts)) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        if statuses.is_empty() {
            return String::new();
        }

        let status = statuses.get(0).unwrap().status();

        if status.is_index_new() || status.is_index_modified() || status.is_index_deleted() {
            "staged".to_string()
        } else if status.is_wt_modified() {
            "modified".to_string()
        } else if status.is_wt_new() {
            "untracked".to_string()
        } else {
            String::new()
        }
    }

    pub fn repository(&self) -> &Repository {
        &self.repo
    }
}
