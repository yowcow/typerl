#![forbid(unsafe_code)]

mod ast;
mod checker;
mod codegen;
mod diag;
mod lexer;
mod modules;
mod parser;

use ast::FileKind;
use diag::{Diag, Span};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: typerl build <file.tpm|file.tpr>...";

/// The compile pipeline recurses over the AST; the parser bounds its height (MAX_DEPTH 64 x
/// MAX_CHAIN 256 reaches ~48k nested expressions), and this big stack gives the checker,
/// generator and drop glue headroom over that bound. Measured: a debug build needs ~200 MiB for
/// that worst case, a release build ~45 MiB; only touched pages are committed.
const STACK_SIZE: usize = 512 * 1024 * 1024;

fn main() -> ExitCode {
    // Never fall back to a smaller, unmeasured stack: report and stop instead.
    match std::thread::Builder::new().stack_size(STACK_SIZE).spawn(run) {
        Ok(handle) => handle.join().unwrap_or(ExitCode::from(101)),
        Err(e) => {
            eprintln!("typerl:1:1: error: cannot start the compiler thread ({} MiB stack): {e}", STACK_SIZE >> 20);
            ExitCode::from(1)
        }
    }
}

fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args[0] != "build" {
        eprintln!("{USAGE}");
        return ExitCode::from(1);
    }
    let mut outputs = Vec::new();
    let mut failed = false;
    for path in &args[1..] {
        match compile(path) {
            Ok(o) => outputs.push(o),
            Err(d) => {
                eprintln!("{d}");
                failed = true;
            }
        }
    }
    if failed {
        return ExitCode::from(1);
    }
    for (out, text, kind) in outputs {
        if let Err(e) = write_output(&out, &text, kind) {
            eprintln!("{}:1:1: error: cannot write file: {e}", out.display());
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn compile(path: &str) -> Result<(PathBuf, String, FileKind), Diag> {
    let kind = match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("tpm") => FileKind::Module,
        Some("tpr") => FileKind::Script,
        _ => return Err(Diag::new(path, Span::START, "expected a .tpm or .tpr file")),
    };
    let src = std::fs::read_to_string(path)
        .map_err(|e| Diag::new(path, Span::START, format!("cannot read file: {e}")))?;
    let file = parser::parse(path, &src, kind)?;
    let facts = checker::check(&file, path)?;
    let ext = match kind {
        FileKind::Module => "pm",
        FileKind::Script => "pl",
    };
    Ok((Path::new(path).with_extension(ext), codegen::generate(&file, &facts), kind))
}

fn write_output(out: &Path, text: &str, kind: FileKind) -> std::io::Result<()> {
    std::fs::write(out, text)?;
    if kind == FileKind::Script {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
