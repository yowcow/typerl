use crate::ast::{File, FileKind};
use crate::diag::{Diag, Span};
use crate::modules::{self, Loader};
use std::collections::HashSet;

/// Facts the generator needs from the checker.
#[derive(Debug, Default)]
pub struct Facts {
    /// Spans of `my` statements that narrow Any to a legacy class (need a runtime check).
    pub narrow: HashSet<Span>,
}

pub fn check(file: &File, path: &str) -> Result<Facts, Diag> {
    if file.kind == FileKind::Module {
        modules::check_package(file, path)?;
    }
    let mut loader = Loader::default();
    for u in &file.uses {
        loader.get(&u.name)?;
    }
    let _own = modules::module_sig(file, path)?;
    // Sub bodies and top-level statements are checked in Task 10.
    Ok(Facts::default())
}
