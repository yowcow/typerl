use crate::ast::File;
use crate::diag::{Diag, Span};
use std::collections::HashSet;

/// Facts the generator needs from the checker.
#[derive(Debug, Default)]
pub struct Facts {
    /// Spans of `my` statements that narrow Any to a legacy class (need a runtime check).
    pub narrow: HashSet<Span>,
}

pub fn check(_file: &File, _path: &str) -> Result<Facts, Diag> {
    Ok(Facts::default())
}
