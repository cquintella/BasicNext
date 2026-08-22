#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Position {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
}

impl SourceFile {
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }

    #[must_use]
    pub fn line(&self, number: usize) -> &str {
        self.text
            .lines()
            .nth(number.saturating_sub(1))
            .unwrap_or("")
    }
}
