use self_update::cargo_crate_version;
use std::io;

pub fn run_upgrade() -> io::Result<()> {
    println!("Checking for updates...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner("sstrelsov")
        .repo_name("marko")
        .bin_name("marko")
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
