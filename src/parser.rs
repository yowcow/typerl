use crate::ast::{File, FileKind};
use crate::diag::{Diag, Span};

pub fn parse(file: &str, _src: &str, _kind: FileKind) -> Result<File, Diag> {
    Err(Diag::new(file, Span::START, "parser not implemented"))
}
