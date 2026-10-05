mod common;
use common::*;
use std::path::PathBuf;

fn build_fixture(targets: &[&str]) -> PathBuf {
    let dir = fixture("run");
    let mut args = vec!["build"];
    args.extend_from_slice(targets);
    assert_ok(&run_typerl(&dir, &args));
    dir
}

#[test]
fn generated_module_runs_with_entry_checks_and_exact_class() {
    let dir = build_fixture(&["Point.tpm", "Point/Label.tpm", "Math.tpm"]);
    let out = perl(&dir, "entry.pl");
    assert_eq!(out.code, 0, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
}

#[test]
fn generated_script_runs_end_to_end() {
    let dir = build_fixture(&["Point.tpm", "Point/Label.tpm", "Math.tpm", "main.tpr"]);
    let out = perl(&dir, "main.pl");
    assert_eq!(out.code, 0, "stderr:\n{}", out.stderr);
    assert_eq!(out.stdout, "1\nlabel=origin\n7\nname=legacy\nlegacy\n0\n1\n2\n3\nthree\n43\n");
}

#[test]
fn legacy_class_annotation_rejects_subclass_at_runtime() {
    let dir = build_fixture(&["bad_child.tpr"]);
    let out = perl(&dir, "bad_child.pl");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("did not pass type constraint"), "{}", out.stderr);
    assert!(!out.stdout.contains("unreachable"));
}

#[test]
fn to_int_rejects_non_integer_at_runtime() {
    let dir = build_fixture(&["bad_int.tpr"]);
    let out = perl(&dir, "bad_int.pl");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("did not pass type constraint"), "{}", out.stderr);
    assert!(!out.stdout.contains("unreachable"));
}

#[test]
fn negated_single_letter_call_is_negation_not_file_test() {
    let dir = tmpdir();
    write(
        &dir,
        "neg.tpr",
        "sub f(Int $x) -> Int {\n    return $x;\n}\n\nmy Int $y = -f(3);\nif ($y == -3) {\n} else {\n    die \"file test\";\n}\n",
    );
    assert_ok(&run_typerl(&dir, &["build", "neg.tpr"]));
    let out = perl(&dir, "neg.pl");
    assert_eq!(out.code, 0, "stderr:\n{}", out.stderr);
}
