use self_update::cargo_crate_version;
use std::io;
use std::sync::mpsc;
use std::thread;

pub fn run_upgrade() -> io::Result<()> {
    println!("Checking for updates...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner("sstrelsov")
        .repo_name("marko")
        .bin_name("marko")
        .bin_path_in_archive("marko-{{ target }}/{{ bin }}")
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .build()
        .and_then(|u| u.update())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let current = cargo_crate_version!();
    if status.version() == current {
        println!("Already up to date (v{current}).");
    } else {
        println!("Updated to v{}.", status.version());
    }

    Ok(())
}

/// Returns a receiver that will eventually contain an update message
/// if a newer version is available on GitHub. Non-blocking — spawns
/// a background thread that hits the GitHub API.
pub fn check_for_update_async() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let current = cargo_crate_version!();
        let latest = self_update::backends::github::Update::configure()
            .repo_owner("sstrelsov")
            .repo_name("marko")
            .bin_name("marko")
            .current_version(current)
            .build()
            .and_then(|u| u.get_latest_release());

        if let Ok(release) = latest {
            let latest_ver = release.version.trim_start_matches('v');
            if latest_ver != current {
                let _ = tx.send(format!(
                    "Update available: v{current} → v{latest_ver} (run `marko upgrade`)"
                ));
            }
        }
        // On error or up-to-date: silently do nothing
    });

    rx
}
