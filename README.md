# Marko

An open source terminal markdown editor written in Rust.

![marko-gif](assets/marko-readme.gif)

## Features

- Dual-pane editor + live preview (Tab to
  switch)
- Syntax highlighting for code blocks
- Git integration (branch, file status,
  diff gutter marks)
- Mouse support (click, drag-select,
  double/triple-click)
- Smart markdown editing (list
  continuation, bracket auto-close, table
  formatting)
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
curl -fsSL
https://github.com/sstrelsov/marko/releases/
latest/download/marko-installer.sh | sh
```

### From source

```bash
cargo install --path .
```

## Usage

```bash
marko <file.md>
```

Creates the file if it doesn't exist.

## Keybindings

### Global

| Key        | Action                                            |
| ---------- | ------------------------------------------------- |
| Tab        | Switch mode (editor / preview)                    |
| Ctrl+S     | Save                                              |
| Ctrl+Q     | Quit                                              |
| Esc        | Back to editor                                    |
| Ctrl+T     | Rename file                                       |
| F1         | Help                                              |

### Editor

| Key                 | Action                                   |
| ------------------- | ---------------------------------------- |
| Ctrl+Z / Ctr        | Undo / Redo                              |
| Ctrl+A              | Select all                               |
| Ctrl+L              | Go to line start                         |
| Ctrl+C / Ctr        | Copy / Paste (system clip                |
| Ctrl+H              | Delete word before cursor                |
| Ctrl+D              | Delete word after cursor                 |
| Ctrl+K              | Delete to end of line                    |

### Mouse

| Action                            | Effect                     |
| --------------------------------- | -------------------------- |
| Click + drag                      | Select text                |
| Click filename                    | Rename file                |
| Click tabs                        | Switch mode                |

## Development

```
cargo run -- <file>
cargo test
```