# Marko

A terminal markdown editor.

<!-- screenshot: ![marko](screenshot.png) -->

## Features

- Dual-pane editor + live preview (Tab to switch)
- Syntax highlighting for code blocks
- Git integration (branch, file status, diff gutter marks)
- Mouse support (click, drag-select, double/triple-click)
- Smart markdown editing (list continuation, bracket auto-close, table formatting)
- System clipboard (copy/paste)
- File rename (Ctrl+T)
- Help overlay (F1)

## Install

### Homebrew (macOS)

```bash
brew install sstrelsov/tap/marko
```

### Shell installer (macOS / Linux)

```bash
curl -fsSL https://github.com/sstrelsov/marko/releases/latest/download/marko-installer.sh | sh
```

### From source

```bash
cargo install --path .
```

## Usage

```bash
marko <file>
```

Creates the file if it doesn't exist.

## Keybindings

### Global

| Key | Action |
|-----|--------|
| Tab | Switch mode (editor / preview) |
| Ctrl+S | Save |
| Ctrl+Q | Quit |
| Esc | Back to editor |
| Ctrl+T | Rename file |
| F1 | Help |

### Editor

| Key | Action |
|-----|--------|
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+A | Select all |
| Ctrl+L | Go to line start |
| Ctrl+C / Ctrl+V | Copy / Paste (system clipboard) |
| Ctrl+H | Delete word before cursor |
| Ctrl+D | Delete word after cursor |
| Ctrl+K | Delete to end of line |

### Mouse

| Action | Effect |
|--------|--------|
| Click + drag | Select text |
| Click filename | Rename file |
| Click tabs | Switch mode |

## Project Layout

```
src/
├── main.rs                    # CLI entry point, terminal setup
├── app.rs                     # App state, event handling, UI layout
├── lib.rs                     # Public module exports
├── theme.rs                   # Color and style constants
├── components/
│   ├── editor.rs              # Text editor widget
│   ├── preview.rs             # Rendered markdown preview widget
│   ├── header.rs              # Title bar (filename, tabs, git branch)
│   └── status.rs              # Status bar
├── git/
│   ├── repo.rs                # Git repository state (branch, file status)
│   └── diff.rs                # Line-level diff for gutter marks
└── markdown/
    ├── renderer.rs            # Markdown → ratatui spans
    ├── code_highlight.rs      # Syntax highlighting for fenced code blocks
    ├── autocomplete.rs        # List continuation, bracket auto-close
    └── table_format.rs        # Pipe-table alignment
```

## Development

```
cargo run -- <file>
cargo test
```
