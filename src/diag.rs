use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub const START: Span = Span { line: 1, col: 1 };
}

#[derive(Debug)]
pub struct Diag {
    pub file: String,
    pub span: Span,
    pub msg: String,
}

impl Diag {
    pub fn cannot_read(file: &str, why: impl fmt::Display) -> Diag {
        Diag::new(file, Span::START, format!("cannot read file: {why}"))
    }

    pub fn new(file: &str, span: Span, msg: impl Into<String>) -> Diag {
        Diag { file: file.to_string(), span, msg: msg.into() }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}:{}: error: {}", self.file, self.span.line, self.span.col, self.msg)
    }
}
