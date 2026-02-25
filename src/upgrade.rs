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
        .bin_path_in_archive("marko-md-{{ target }}/{{ bin }}")
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
/// a background thread that checks the latest release via redirect.
pub fn check_for_update_async() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let current = cargo_crate_version!();

        // Follow the /releases/latest redirect to get the tag — doesn't hit API rate limit
        let output = std::process::Command::new("curl")
            .args([
                "-sI", "-o", "/dev/null",
                "-w", "%{redirect_url}",
                "https://github.com/sstrelsov/marko/releases/latest",
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout);
                // URL looks like: https://github.com/sstrelsov/marko/releases/tag/v0.1.4
                if let Some(tag_pos) = url.rfind("/v") {
                    let tag = &url[tag_pos + 2..];
                    let tag = tag.trim();
                    if !tag.is_empty() && tag != current {
                        let _ = tx.send(format!(
                            "v{tag} available — run marko upgrade"
                        ));
                    }
                }
            }
        }
        // On error or up-to-date: silently do nothing
    });

    rx
}
