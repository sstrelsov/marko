use std::io;
use std::panic;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use marko::{app, pandoc, upgrade};

#[derive(Parser)]
#[command(name = "marko", version, about = "A terminal markdown editor")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// File to open for editing
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Export markdown to .docx
    Export {
        /// Markdown file to export
        file: PathBuf,
        /// Output .docx path (defaults to same name with .docx extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Reference .docx for styling (passed as --reference-doc to pandoc)
        #[arg(long)]
        reference_doc: Option<PathBuf>,
    },
    /// Update marko to the latest version
    Upgrade,
}

fn main() -> io::Result<()> {
    marko::markdown::code_highlight::ensure_loaded();

    let cli = Cli::parse();

    // Handle subcommands first
    match cli.command {
        Some(Commands::Export {
            file,
            output,
            reference_doc,
        }) => return handle_export(&file, output.as_deref(), reference_doc.as_deref()),
        Some(Commands::Upgrade) => return upgrade::run_upgrade(),
        None => {}
    }

    // No subcommand — must have a file argument
    let file = match cli.file {
        Some(f) => f,
        None => {
            eprintln!("Usage: marko <FILE> or marko export <FILE>");
            std::process::exit(1);
        }
    };

    // Detect .docx files — import via pandoc
    let is_docx = file
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("docx"))
        .unwrap_or(false);

    if is_docx {
        return handle_docx_open(&file);
    }

    // Regular .md file — existing flow
    if !file.exists() {
        std::fs::write(&file, "")?;
    }
    let file_path = file.canonicalize()?;

    run_editor(file_path, None)
}

/// Handles `marko export file.md` — converts to .docx and exits.
fn handle_export(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    reference_doc: Option<&std::path::Path>,
) -> io::Result<()> {
    if !pandoc::is_available() {
        eprintln!("Error: pandoc is not installed.");
        eprintln!("Install it from https://pandoc.org/installing.html");
        std::process::exit(1);
    }

    if !file.exists() {
        eprintln!("Error: file not found: {}", file.display());
        std::process::exit(1);
    }

    let docx_path = match output {
        Some(p) => p.to_path_buf(),
        None => file.with_extension("docx"),
    };

    match pandoc::md_to_docx(file, &docx_path, reference_doc) {
        Ok(_) => {
            println!("Exported to {}", docx_path.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("Export failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handles opening a .docx file: converts to .md, then opens the editor with docx state.
fn handle_docx_open(docx_file: &PathBuf) -> io::Result<()> {
    if !pandoc::is_available() {
        eprintln!("Error: pandoc is not installed.");
        eprintln!("Install it from https://pandoc.org/installing.html");
        std::process::exit(1);
    }

    if !docx_file.exists() {
        eprintln!("Error: file not found: {}", docx_file.display());
        std::process::exit(1);
    }

    let docx_path = docx_file.canonicalize()?;
    let md_path = docx_path.with_extension("md");

    // Convert .docx → markdown
    let markdown = match pandoc::docx_to_md(&docx_path) {
        Ok(md) => md,
        Err(e) => {
            eprintln!("Failed to convert .docx to markdown: {}", e);
            std::process::exit(1);
        }
    };

    // Write sibling .md file
    std::fs::write(&md_path, &markdown)?;

    let docx_state = app::DocxState {
        docx_path: docx_path.clone(),
        reference_doc: docx_path,
    };

    run_editor(md_path, Some(docx_state))
}

/// Sets up the terminal, runs the TUI editor, and restores the terminal on exit.
fn run_editor(file_path: PathBuf, docx_state: Option<app::DocxState>) -> io::Result<()> {
    // Setup panic hook to restore terminal
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Run app
    let result = run_app(&mut terminal, file_path, docx_state);

    // Restore terminal
    restore_terminal()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    file_path: PathBuf,
    docx_state: Option<app::DocxState>,
) -> io::Result<()> {
    let mut app = app::App::new(file_path);

    if let Some(ds) = docx_state {
        let docx_name = ds
            .docx_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document.docx")
            .to_string();
        app.docx_state = Some(ds);
        app.set_status(&format!("Opened {} (editing as markdown)", docx_name));
    }

    loop {
        app.render_frame(terminal)?;

        // Block up to 100ms waiting for the first event (prevents busy-loop,
        // gives tick() a chance to run ~10x/sec for timer expiry).
        if event::poll(Duration::from_millis(100))? {
            // Drain all queued events without blocking, then render immediately.
            loop {
                let ev = event::read()?;
                app.handle_event(ev);
                if app.should_quit {
                    break;
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    Ok(())
}
