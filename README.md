# Rustik - Vim like Modal Text Editor

This project is a Vim-like modal text editor written in Rust, started as a fun project to learn both Vim and the Rust language. It is designed to provide a lightweight yet powerful text editing experience, combining the efficiency of Vim keybindings with modern syntax highlighting, themes, and LSP diagnostics.

## Features

- **Vim Motions**: Navigate through the file using Vim-style commands.

- **Syntax Highlighting**: Tree-sitter is integrated for precise syntax highlighting, making it easier to work with code files.

- **Themes**: Uses VSCode themes by default. The Catppuccin theme is the default, but more themes can be added easily.

- **LSP Support (Coming Soon)**: Basic LSP (Language Server Protocol) diagnostics integration to show errors and warnings in your code.

- **Multi-buffer Support (Coming Soon)**: Work with multiple files at the same time, similar to buffers in Vim.

- **File Picker (Coming Soon)**: Easily open files without leaving the editor.

## Project Status

This project is currently in development, with many of the core features implemented. There are several planned features such as multi-buffer support, file picker, and extended theme options. Contributions and suggestions are welcome!

[Click here to watch the demo video](https://drive.google.com/file/d/10DU_4BM3aqpp7F6TGNExJQGmkhV3hXQY/view?usp=sharing)

## Installation

To get started with this project, follow these steps:

### Prerequisites

- Rust
- Git
- A terminal emulator with 256 color support

### Clone the Repository

```bash
git clone https://github.com/ParthSingh0306/rustik.git
cd rustik
cargo build --release
cargo run -- your_file_path
```

## Keybindings

This editor operates in a modal fashion, similar to Vim, with different keybindings based on the current mode:

### Normal Mode

- `gg` - Move to the top of the file
- `G` - Move to the bottom of the file
- `dd` - Delete the current line
- `u` - Undo the last change
- `x` - Remove the current character
- `zz`- Center the current line on the screen
- `$` - To go to the end of current line
- `0` - To got to the start of the current line
- `h` or `←` - Move cursor left
- `j` or `↓` - Move cursor down
- `k` or `↑` - Move cursor up
- `l` or `→` - Move cursor right
- more to be Implemented

### Insert Mode

- `i` - Enter Insert Mode to begin editing text
- `Esc` or `q` - Return to Normal Mode

### Future Features

- **Visual Mode (Planned)**: A mode for selecting and manipulating blocks of text
- **Command Mode (Planned)**: A mode for executing commands.
- **LSP Support (Planned)**: Basic LSP (Language Server Protocol) diagnostics integration to show errors and warnings in your code.
- **Multi-buffer Support (Planned)**: Work with multiple files at the same time, similar to buffers in Vim.
- **File Picker (Planned)**: Easily open files without leaving the editor.
