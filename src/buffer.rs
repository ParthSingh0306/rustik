pub struct Buffer {
    pub file: Option<String>,
    pub lines: Vec<String>,
}

impl Buffer {
    pub fn from_file(file: Option<String>) -> Self {
        let lines = match &file {
            Some(file) => {
                let file = std::fs::read_to_string(file).unwrap();
                file.lines().map(|s| s.to_string()).collect()
            }
            None => vec![],
        };

        Self { file, lines }
    }
}
