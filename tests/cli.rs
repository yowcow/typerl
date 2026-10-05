mod common;
use common::*;

#[test]
fn usage_without_arguments() {
    let out = run_typerl(&tmpdir(), &[]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("usage: typerl build <file.tpm|file.tpr>..."), "{}", out.stderr);
}

#[test]
fn usage_with_unknown_subcommand() {
    let out = run_typerl(&tmpdir(), &["compile", "a.tpr"]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("usage: typerl build"), "{}", out.stderr);
}

#[test]
fn rejects_unknown_extension() {
    assert_err(&build(&[("a.txt", "")], &["a.txt"]), "a.txt:1:1:", "expected a .tpm or .tpr file");
}

#[test]
fn rejects_missing_file() {
    assert_err(&build(&[], &["nope.tpr"]), "nope.tpr:1:1:", "cannot read file");
}

#[test]
fn script_builds_to_pl() {
    let out = script("my Int $x = 1;\n");
    assert_ok(&out);
    assert!(read_output(&out, "a.tpr").starts_with("#!/usr/bin/env perl\n"));
}

#[test]
fn script_output_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let out = script("my Int $x = 1;\n");
    assert_ok(&out);
    let mode = std::fs::metadata(out.dir.join("a.pl")).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111);
}

#[test]
fn module_builds_to_pm_ending_with_1() {
    let out = build(&[("Point.tpm", "package Point;\n")], &["Point.tpm"]);
    assert_ok(&out);
    assert!(read_output(&out, "Point.tpm").ends_with("\n1;\n"));
}

#[test]
fn nested_module_builds_next_to_source() {
    assert_ok(&build(&[("Point/Label.tpm", "package Point::Label;\n")], &["Point/Label.tpm"]));
}

#[test]
fn success_prints_nothing() {
    let out = script("my Int $x = 1;\n");
    assert_ok(&out);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "");
}

#[test]
fn diagnostic_format_is_file_line_col() {
    let out = script("my Int $x = 1;\nmy Str $s = 1;\n");
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "a.tpr:2:13: error: type mismatch: expected Str, found Int\n");
    assert!(!out.dir.join("a.pl").exists());
}

#[test]
fn builds_multiple_files() {
    assert_ok(&build(
        &[("one.tpr", "my Int $x = 1;\n"), ("two.tpr", "my Int $y = 2;\n")],
        &["one.tpr", "two.tpr"],
    ));
}

#[test]
fn any_failure_writes_no_outputs() {
    let out = build(
        &[("ok.tpr", "my Int $x = 1;\n"), ("bad.tpr", "my Str $s = 1;\n")],
        &["ok.tpr", "bad.tpr"],
    );
    assert_err(&out, "bad.tpr:1:13:", "type mismatch");
}

#[test]
fn reports_each_failing_file() {
    let out = build(
        &[("bad1.tpr", "my Str $s = 1;\n"), ("bad2.tpr", "my Int $i = \"a\";\n")],
        &["bad1.tpr", "bad2.tpr"],
    );
    assert_err(&out, "bad1.tpr:1:", "type mismatch");
    assert_err(&out, "bad2.tpr:1:", "type mismatch");
}
