#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub dir: PathBuf,
    pub targets: Vec<String>,
}

/// Fresh empty directory under cargo's per-test tmp dir (inside target/, never in the repo tree).
pub fn tmpdir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "case-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn write(dir: &Path, rel: &str, src: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, src).unwrap();
}

/// Runs the typerl binary with `dir` as the current directory.
pub fn run_typerl(dir: &Path, args: &[&str]) -> Out {
    let o = Command::new(env!("CARGO_BIN_EXE_typerl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run typerl");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        dir: dir.to_path_buf(),
        targets: args.iter().skip(1).map(|s| s.to_string()).collect(),
    }
}

/// Writes `files` into a fresh dir and runs `typerl build <targets>` there.
pub fn build(files: &[(&str, &str)], targets: &[&str]) -> Out {
    let dir = tmpdir();
    for (rel, src) in files {
        write(&dir, rel, src);
    }
    let mut args = vec!["build"];
    args.extend_from_slice(targets);
    run_typerl(&dir, &args)
}

/// Builds a single script `a.tpr`.
pub fn script(src: &str) -> Out {
    build(&[("a.tpr", src)], &["a.tpr"])
}

pub fn output_path(target: &str) -> String {
    if let Some(s) = target.strip_suffix(".tpm") {
        format!("{s}.pm")
    } else if let Some(s) = target.strip_suffix(".tpr") {
        format!("{s}.pl")
    } else {
        format!("{target}.out")
    }
}

pub fn read_output(out: &Out, target: &str) -> String {
    let p = out.dir.join(output_path(target));
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

pub fn assert_ok(out: &Out) {
    assert_eq!(out.code, 0, "expected success; stderr:\n{}", out.stderr);
    for t in &out.targets {
        assert!(out.dir.join(output_path(t)).exists(), "missing output for {t}");
    }
}

fn is_diag(line: &str) -> bool {
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    parts.len() == 4
        && parts[1].parse::<u32>().is_ok()
        && parts[2].parse::<u32>().is_ok()
        && parts[3].starts_with(" error: ")
}

/// Exit 1, every stderr line is a diagnostic, one starts with `prefix` and contains `needle`,
/// and no output file exists for any target.
pub fn assert_err(out: &Out, prefix: &str, needle: &str) {
    assert_eq!(out.code, 1, "expected exit 1; stderr:\n{}", out.stderr);
    for l in out.stderr.lines() {
        assert!(is_diag(l), "not a `file:line:col: error: msg` line: {l:?}");
    }
    assert!(
        out.stderr.lines().any(|l| l.starts_with(prefix) && l.contains(needle)),
        "no diagnostic starting with {prefix:?} containing {needle:?}; stderr:\n{}",
        out.stderr
    );
    for t in &out.targets {
        assert!(!out.dir.join(output_path(t)).exists(), "output for {t} must not be written on failure");
    }
}

pub fn rejects(src: &str, prefix: &str, needle: &str) {
    assert_err(&script(src), prefix, needle);
}

pub fn rejects_module(path: &str, src: &str, prefix: &str, needle: &str) {
    assert_err(&build(&[(path, src)], &[path]), prefix, needle);
}

fn copy_dir(from: &Path, to: &Path) {
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.path().is_dir() {
            fs::create_dir_all(&dst).unwrap();
            copy_dir(&e.path(), &dst);
        } else {
            fs::copy(e.path(), &dst).unwrap();
        }
    }
}

/// Copies tests/fixtures/<name> into a fresh dir.
pub fn fixture(name: &str) -> PathBuf {
    let dir = tmpdir();
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name), &dir);
    dir
}

/// Runs `perl -I. <script>` in `dir`.
pub fn perl(dir: &Path, script: &str) -> Out {
    let o = Command::new("perl").arg("-I.").arg(script).current_dir(dir).output().expect("run perl");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        dir: dir.to_path_buf(),
        targets: vec![],
    }
}
