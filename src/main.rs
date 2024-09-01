use buffer::Buffer;
use editor::Editor;

mod buffer;
mod editor;

fn main() -> anyhow::Result<()> {
    let file = std::env::args().nth(1);
    let buffer = Buffer::from_file(file);

    let mut editor = Editor::new(buffer)?;
    let _ = editor.run();
    Ok(())
}
