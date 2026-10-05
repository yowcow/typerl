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

#[test]
fn negated_method_chain_rooted_at_file_test_letter_is_negation_not_file_test() {
    let dir = tmpdir();
    write(
        &dir,
        "C.tpm",
        "package C;\n\nfield x: Int;\n\nsub new(Class $class, :Int $x) -> C {\n    return bless({ x => $x }, $class);\n}\n\nsub x(C $self) -> Int {\n    return $self->{x};\n}\n",
    );
    write(
        &dir,
        "neg.tpr",
        "use C;\n\nsub f(Int $n) -> C {\n    return C->new(x => $n);\n}\n\nmy Int $a = -f(3)->x;\nmy Int $b = -C->new(x => 4)->x();\nif ($a == -3) {\n} else {\n    die \"file test a\";\n}\nif ($b == -4) {\n} else {\n    die \"file test b\";\n}\n",
    );
    assert_ok(&run_typerl(&dir, &["build", "C.tpm", "neg.tpr"]));
    let out = perl(&dir, "neg.pl");
    assert_eq!(out.code, 0, "stderr:\n{}", out.stderr);
}

#[test]
fn interface_dispatch_runs_through_perl() {
    let dir = tmpdir();
    write(&dir, "Shape.tpm", "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\n");
    write(
        &dir,
        "Circle.tpm",
        "package Circle;\nfield r: Int;\nsub new(Class $class, :Int $r) -> Circle {\n    return bless({ r => $r }, $class);\n}\nsub area(Circle $self) -> Int {\n    return $self->{r} * $self->{r};\n}\n",
    );
    write(
        &dir,
        "iface_main.tpr",
        "use Shape;\nuse Circle;\nsub report(Shape $s) -> Int {\n    return $s->area;\n}\nmy Circle $c = Circle->new(r => 3);\nmy Int $a = report($c);\nif ($a == 9) {\n} else {\n    die \"bad area\";\n}\n",
    );
    assert_ok(&run_typerl(&dir, &["build", "Shape.tpm", "Circle.tpm", "iface_main.tpr"]));
    let out = perl(&dir, "iface_main.pl");
    assert_eq!(out.code, 0, "stderr:\n{}", out.stderr);
}
