pub struct Buffer {
    pub file: Option<String>,
    pub lines: Vec<String>,
}

impl Buffer {
    pub fn new(file: Option<String>, contents: String) -> Self {
        let lines = contents.lines().map(|s| s.to_string()).collect();
        Self { file, lines }
    }

    pub fn from_file(file: Option<String>) -> Self {
        match &file {
            Some(file) => {
                let contents = std::fs::read_to_string(file).unwrap();
                Self::new(Some(file.to_string()), contents.to_string())
            }
            None => Self::new(file, String::new()),
        }
    }

    pub fn get(&self, line: usize) -> Option<String> {
        if self.lines.len() > line {
            return Some(self.lines[line].clone());
        }

        None
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn insert(&mut self, x: u16, y: usize, c: char) {
        if let Some(line) = self.lines.get_mut(y as usize) {
            (*line).insert(x as usize, c);
        }
    }

    pub fn insert_line(&mut self, line: usize, content: String) {
        self.lines.insert(line, content);
    }

    pub fn remove(&mut self, x: u16, y: usize) {
        if let Some(line) = self.lines.get_mut(y as usize) {
            (*line).remove(x as usize);
        }
    }

    pub fn remove_line(&mut self, line: usize) {
        if self.len() > line {
            self.lines.remove(line);
        }
    }

    pub(crate) fn viewport(&self, vtop: usize, vheight: usize) -> String {
        let height = std::cmp::min(vtop + vheight, self.lines.len());
        self.lines[vtop..height].join("\n")
    }
}

#[cfg(test)]

mod test {
    use super::*;

    #[test]
    fn test_viewport() {
        let buffer = Buffer::new(Some("sample.txt".to_string()), "a\nb".to_string());
        assert_eq!(buffer.viewport(0, 5), "a\nb".to_string());
    }

    #[test]
    fn test_viewport_with_small_buffer() {
        let buffer = Buffer::new(
            Some("sample.txt".to_string()),
            "fn main() {\n    println!(\"Hello, world!\");\n    }".to_string(),
        );
        assert_eq!(
            buffer.viewport(0, 2),
            "fn main() {\n    println!(\"Hello, world!\");".to_string()
        );
    }
}
