mod common;
use common::*;

#[test]
fn rejects_package_name_mismatch() {
    rejects_module("Point.tpm", "package Pointe;\n", "Point.tpm:1:", "package `Pointe` does not match the file name (expected `Point`)");
}

#[test]
fn rejects_nested_package_name_mismatch() {
    rejects_module("Point/Label.tpm", "package Label;\n", "Point/Label.tpm:1:", "expected `Point::Label`");
}

#[test]
fn accepts_nested_package_name() {
    assert_ok(&build(&[("Point/Label.tpm", "package Point::Label;\n")], &["Point/Label.tpm"]));
}

#[test]
fn accepts_dot_slash_module_path() {
    assert_ok(&build(&[("Point.tpm", "package Point;\n")], &["./Point.tpm"]));
}

#[test]
fn rejects_absolute_module_path() {
    let dir = tmpdir();
    write(&dir, "Point.tpm", "package Point;\n");
    let abs = dir.join("Point.tpm").to_string_lossy().into_owned();
    let out = run_typerl(&dir, &["build", &abs]);
    assert_err(&out, &abs, "module paths must be relative to the current directory");
}

#[test]
fn rejects_used_module_with_syntax_error() {
    let out = build(
        &[("Point/Label.tpm", "package Point::Label;\nmy Int $x = 1;\n"), ("a.tpr", "use Point::Label;\n")],
        &["a.tpr"],
    );
    assert_err(&out, "Point/Label.tpm:2:", "module top level may only contain");
}

#[test]
fn rejects_used_module_package_mismatch() {
    let out = build(&[("Point/Label.tpm", "package Label;\n"), ("a.tpr", "use Point::Label;\n")], &["a.tpr"]);
    assert_err(&out, "Point/Label.tpm:1:", "expected `Point::Label`");
}

#[test]
fn rejects_used_module_bad_signature() {
    let out = build(
        &[("Point.tpm", "package Point;\nsub f(Int $a, :Int $b) -> Int {\n    return $a;\n}\n"), ("a.tpr", "use Point;\n")],
        &["a.tpr"],
    );
    assert_err(&out, "Point.tpm:2:", "positional and named parameters cannot be mixed");
}

#[test]
fn missing_tpm_means_legacy_perl() {
    assert_ok(&script("use No::Such;\nmy Any $v = No::Such::f();\nmy Any $w = No::Such->new();\n"));
}

#[test]
fn rejects_mixed_positional_and_named_params() {
    rejects("sub f(Int $a, :Int $b) -> Int {\n    return $a;\n}\n", "a.tpr:1:", "positional and named parameters cannot be mixed");
}

#[test]
fn rejects_positional_constructor_params() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nsub new(Class $class, Int $x) -> Point {\n    return bless({ x => $x }, $class);\n}\n",
        "Point.tpm:3:",
        "constructor parameters after `$class` must be named",
    );
}

#[test]
fn rejects_constructor_returning_other_type() {
    rejects_module("Point.tpm", "package Point;\nsub new(Class $class) -> Int {\n    return 1;\n}\n", "Point.tpm:2:", "constructor must return Point");
}

#[test]
fn rejects_method_with_wrong_invocant_type() {
    rejects_module("Point.tpm", "package Point;\nsub x(Int $self) -> Int {\n    return 1;\n}\n", "Point.tpm:2:", "method invocant `$self` must have type Point");
}

#[test]
fn rejects_constructor_invocant_without_class_type() {
    rejects_module("Point.tpm", "package Point;\nsub new(Int $class) -> Point {\n    return 1;\n}\n", "Point.tpm:2:", "constructor invocant `$class` must have type Class");
}

#[test]
fn rejects_invocant_not_first() {
    rejects_module("Point.tpm", "package Point;\nsub f(Int $a, Point $self) -> Int {\n    return 1;\n}\n", "Point.tpm:2:", "`$self` must be the first parameter");
}

#[test]
fn rejects_class_type_outside_constructor_invocant() {
    rejects_module(
        "Point.tpm",
        "package Point;\nsub f(Class $c) -> Int {\n    return 1;\n}\n",
        "Point.tpm:2:",
        "Class is only allowed as the type of a constructor's first parameter `$class`",
    );
}

#[test]
fn rejects_class_typed_field() {
    rejects_module("Point.tpm", "package Point;\nfield c: Class;\n", "Point.tpm:2:", "Class is only allowed");
}

#[test]
fn rejects_constructor_and_method_in_script() {
    rejects("sub new(Class $class) -> Int {\n    return 1;\n}\n", "a.tpr:1:", "constructors and methods are only allowed in modules (.tpm)");
    rejects("sub get(Int $self) -> Int {\n    return 1;\n}\n", "a.tpr:1:", "constructors and methods are only allowed in modules (.tpm)");
}

#[test]
fn rejects_duplicate_declarations() {
    rejects("sub f() -> Int {\n    return 1;\n}\nsub f() -> Int {\n    return 2;\n}\n", "a.tpr:4:", "duplicate sub `f`");
    rejects("sub f(Int $a, Int $a) -> Int {\n    return 1;\n}\n", "a.tpr:1:", "duplicate parameter `$a`");
    rejects_module("Point.tpm", "package Point;\nfield x: Int;\nfield x: Str;\n", "Point.tpm:3:", "duplicate field `x`");
}

#[test]
fn rejects_reserved_sub_names() {
    for name in ["Int", "Maybe", "InstanceOf", "to_str", "import", "DESTROY", "AUTOLOAD", "BEGIN"] {
        rejects(&format!("sub {name}() -> Int {{\n    return 1;\n}}\n"), "a.tpr:1:", &format!("`{name}` is a reserved name"));
    }
}

#[test]
fn rejects_function_named_like_perl_builtin() {
    rejects("sub join(Str $s) -> Str {\n    return $s;\n}\n", "a.tpr:1:", "`join` is a Perl built-in");
    rejects("sub atan2(Int $a) -> Int {\n    return $a;\n}\n", "a.tpr:1:", "`atan2` is a Perl built-in");
    rejects("sub __END__() -> Int {\n    return 1;\n}\n", "a.tpr:1:", "`__END__` is a Perl built-in");
}

#[test]
fn accepts_method_named_like_perl_builtin() {
    assert_ok(&build(&[("Point.tpm", "package Point;\nsub keys(Point $self) -> Int {\n    return 1;\n}\nsub __PACKAGE__(Point $self) -> Int {\n    return 1;\n}\n")], &["Point.tpm"]));
}

#[test]
fn rejects_void_parameter() {
    rejects("sub f(Void $x) -> Int {\n    return 1;\n}\n", "a.tpr:1:", "Void is only allowed as a return type");
}

#[test]
fn rejects_unknown_type_in_signature() {
    rejects("sub f(Foo $x) -> Int {\n    return 1;\n}\n", "a.tpr:1:", "unknown type `Foo`");
}

#[test]
fn accepts_used_legacy_class_in_signature() {
    assert_ok(&script("use Foo;\nsub f(Foo $x) -> Int {\n    return 1;\n}\n"));
}

// ---- Task 12: only a genuinely missing .tpm is legacy Perl ----

#[test]
fn directory_named_like_module_is_an_error() {
    let dir = tmpdir();
    write(&dir, "Foo.tpm/keep", "");
    write(&dir, "a.tpr", "use Foo;\n");
    let out = run_typerl(&dir, &["build", "a.tpr"]);
    assert_err(&out, "Foo.tpm:1:1:", "not a regular file");
}

#[test]
fn dangling_symlink_module_is_an_error() {
    let dir = tmpdir();
    std::os::unix::fs::symlink("missing-target", dir.join("Foo.tpm")).unwrap();
    write(&dir, "a.tpr", "use Foo;\n");
    let out = run_typerl(&dir, &["build", "a.tpr"]);
    assert_err(&out, "Foo.tpm:1:1:", "cannot read");
}

#[test]
fn symlink_to_regular_module_is_followed() {
    let dir = tmpdir();
    write(&dir, "Real.tpm", "package Foo;\n\nsub f() -> Int {\n    return 1;\n}\n");
    std::os::unix::fs::symlink("Real.tpm", dir.join("Foo.tpm")).unwrap();
    write(&dir, "a.tpr", "use Foo;\nmy Int $n = Foo::f();\n");
    assert_ok(&run_typerl(&dir, &["build", "a.tpr"]));
}

#[test]
fn missing_module_is_legacy_perl() {
    assert_ok(&script("use Foo;\nmy Any $x = Foo::f();\n"));
}

#[test]
fn rejects_field_in_interface_module() {
    rejects_module(
        "Shape.tpm",
        "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\nfield x: Int;\n",
        "Shape.tpm:",
        "interface modules cannot declare fields",
    );
}

#[test]
fn rejects_bodied_sub_in_interface_module() {
    rejects_module(
        "Shape.tpm",
        "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\nsub area(Shape $self) -> Int {\n    return 1;\n}\n",
        "Shape.tpm:",
        "interface modules cannot define sub bodies",
    );
}

#[test]
fn rejects_empty_interface_module() {
    rejects_module("Shape.tpm", "package Shape;\ninterface {\n}\n", "Shape.tpm:", "interface must declare at least one method");
}

#[test]
fn rejects_duplicate_method_in_interface() {
    rejects_module(
        "Shape.tpm",
        "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n    sub area(Shape $self) -> Int;\n}\n",
        "Shape.tpm:",
        "duplicate method `area` in interface",
    );
}

#[test]
fn rejects_non_method_in_interface() {
    rejects_module(
        "Shape.tpm",
        "package Shape;\ninterface {\n    sub f(Int $x) -> Int;\n}\n",
        "Shape.tpm:",
        "interface methods must be methods",
    );
}
