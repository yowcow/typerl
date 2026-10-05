mod common;
use common::*;

const MATH: &str = "package Math;\n\nsub add(Int $x, Int $y) -> Int {\n    return $x + $y;\n}\n";

fn with(files: &[(&str, &str)], main: &str) -> Out {
    let mut all = files.to_vec();
    all.push(("a.tpr", main));
    build(&all, &["a.tpr"])
}

// ---- accepted ----

#[test]
fn accepts_any_from_legacy_narrowed_by_to_str() {
    assert_ok(&script(
        "use Legacy::Util;\nmy Any $raw = Legacy::Util::name();\nmy Str $name = to_str($raw);\nmy Int $n = to_int(Legacy::Util::number());\nmy Bool $b = to_bool(Legacy::Util::flag());\n",
    ));
}

#[test]
fn accepts_any_to_any_and_into_legacy() {
    assert_ok(&script(
        "use Legacy::Util;\nmy Any $a = Legacy::Util::name();\nmy Any $b = $a;\nmy Any $c = 1;\nmy ArrayRef[Int] $xs = [1];\nLegacy::Util::emit($b, $xs, 1, \"a\");\nLegacy::Util::emit(k => 1);\n",
    ));
}

#[test]
fn accepts_to_conversions_on_any_type() {
    assert_ok(&script("my Str $s = to_str(1);\nmy Int $n = to_int(\"2\");\n"));
}

#[test]
fn accepts_typed_module_function_call() {
    assert_ok(&with(&[("Math.tpm", MATH)], "use Math;\nmy Int $z = Math::add(1, 2);\n"));
}

#[test]
fn accepts_named_function_args_in_any_order() {
    assert_ok(&script("sub area(:Int $w, :Int $h) -> Int {\n    return $w * $h;\n}\nmy Int $a = area(h => 2, w => 3);\n"));
}

#[test]
fn accepts_omitted_optional_named_arg() {
    assert_ok(&script(
        "sub f(:Int $a, :Optional[Str] $b) -> Int {\n    return $a;\n}\nmy Int $x = f(a => 1);\nmy Int $y = f(b => \"s\", a => 1);\n",
    ));
}

#[test]
fn accepts_union_and_optional() {
    assert_ok(&script(
        "sub pick(Int|Str $v) -> Int|Str {\n    return $v;\n}\nsub maybe(Optional[Int] $v) -> Optional[Int] {\n    return $v;\n}\nmy Int|Str $a = pick(1);\nmy Int|Str $b = pick(\"x\");\nmy Optional[Int] $c = maybe(1);\nmy ArrayRef[Int|Str] $mixed = [1, \"a\"];\nmy Any $any = $a;\n",
    ));
}

#[test]
fn accepts_foreach_over_arrayref() {
    assert_ok(&script(
        "my ArrayRef[Int] $xs = [1, 2];\nforeach my Int $x ($xs) {\n    my Int $y = $x + 1;\n}\nforeach my Int|Str $v ([1, \"a\"]) {\n}\n",
    ));
}

#[test]
fn accepts_interpolation_of_str_vars() {
    assert_ok(&script("sub f(Str $name) -> Str {\n    my Str $greet = \"hi\";\n    return \"$greet, $name!\";\n}\n"));
}

#[test]
fn accepts_forward_and_recursive_calls() {
    assert_ok(&script(
        "my Int $v = later(3);\nsub later(Int $n) -> Int {\n    if ($n == 0) {\n        return 0;\n    }\n    return later($n - 1);\n}\n",
    ));
}

#[test]
fn accepts_die_and_else_as_terminators() {
    assert_ok(&script(
        "sub f(Int $n) -> Int {\n    if ($n == 0) {\n        die \"zero\";\n    } elsif ($n == 1) {\n        return 1;\n    } else {\n        return $n;\n    }\n}\n",
    ));
}

#[test]
fn accepts_void_sub_without_return() {
    assert_ok(&script("sub f(Int $n) -> Void {\n    if ($n == 0) {\n        return;\n    }\n}\nf(1);\n"));
}

// ---- rejected ----

#[test]
fn rejects_int_returned_as_str() {
    rejects("sub f() -> Str {\n    return 1;\n}\n", "a.tpr:2:", "type mismatch: expected Str, found Int");
}

#[test]
fn rejects_sub_without_return() {
    rejects("sub f(Int $x) -> Int {\n    my Int $y = $x;\n}\n", "a.tpr:1:", "sub `f` must end with `return` on every path");
}

#[test]
fn rejects_return_only_in_if_branch() {
    rejects(
        "sub f(Int $x) -> Int {\n    if ($x == 1) {\n        return 1;\n    }\n}\n",
        "a.tpr:1:",
        "sub `f` must end with `return` on every path",
    );
}

#[test]
fn rejects_return_value_in_void_sub() {
    rejects("sub f() -> Void {\n    return 1;\n}\n", "a.tpr:2:", "Void sub cannot return a value");
}

#[test]
fn rejects_bare_return_in_value_sub() {
    rejects("sub f() -> Int {\n    return;\n}\n", "a.tpr:2:", "must return a value of type Int");
}

#[test]
fn rejects_void_value_used() {
    rejects("sub f() -> Void {\n}\nmy Any $v = f();\n", "a.tpr:3:", "Void value cannot be used");
}

#[test]
fn rejects_return_outside_sub() {
    rejects("return 1;\n", "a.tpr:1:", "return outside a sub");
}

#[test]
fn rejects_undeclared_variable() {
    rejects("my Int $x = $y;\n", "a.tpr:1:", "undeclared variable `$y`");
}

#[test]
fn rejects_undeclared_self_and_class() {
    rejects("sub f(Int $x) -> Int {\n    return $self;\n}\n", "a.tpr:2:", "undeclared variable `$self`");
    rejects_module("Point.tpm", "package Point;\nsub f(Int $x) -> Int {\n    return $class;\n}\n", "Point.tpm:3:", "undeclared variable `$class`");
}

#[test]
fn rejects_redeclaration() {
    rejects("my Int $x = 1;\nmy Int $x = 2;\n", "a.tpr:2:", "`$x` is already declared");
    rejects(
        "sub f(Int $x) -> Int {\n    if ($x == 1) {\n        my Int $x = 2;\n    }\n    return $x;\n}\n",
        "a.tpr:3:",
        "`$x` is already declared",
    );
}

#[test]
fn rejects_sub_reading_toplevel_variable() {
    rejects("my Int $g = 1;\nsub f() -> Int {\n    return $g;\n}\n", "a.tpr:3:", "undeclared variable `$g`");
}

#[test]
fn rejects_operator_type_errors() {
    let cases = [
        ("my Int $v = 1 + \"a\";\n", "operator `+` requires Int operands"),
        ("my Int $v = \"a\" * 2;\n", "operator `*` requires Int operands"),
        ("my Int $v = 1 - \"a\";\n", "operator `-` requires Int operands"),
        ("my Str $v = 1 . \"a\";\n", "operator `.` requires Str operands"),
        ("my Bool $v = \"a\" == \"b\";\n", "operator `==` requires Int operands"),
        ("my Bool $v = 1 eq 2;\n", "operator `eq` requires Str operands"),
        ("my Int $v = -\"a\";\n", "unary `-` requires an Int operand"),
    ];
    for (src, needle) in cases {
        rejects(src, "a.tpr:1:", needle);
    }
}

#[test]
fn rejects_non_bool_conditions() {
    rejects("if (1) {\n}\n", "a.tpr:1:", "if condition must be Bool, found Int");
    rejects("if (1 == 1) {\n} elsif (\"a\") {\n}\n", "a.tpr:2:", "if condition must be Bool, found Str");
}

#[test]
fn rejects_any_assigned_to_str() {
    rejects("use Legacy::Util;\nmy Str $s = Legacy::Util::name();\n", "a.tpr:2:", "type mismatch: expected Str, found Any");
}

#[test]
fn rejects_any_passed_to_int_param() {
    rejects(
        "use Legacy::Util;\nsub f(Int $x) -> Int {\n    return $x;\n}\nmy Int $v = f(Legacy::Util::number());\n",
        "a.tpr:5:",
        "argument 1 of f: type mismatch: expected Int, found Any",
    );
}

#[test]
fn rejects_any_in_operator_and_condition() {
    rejects("use Legacy::Util;\nmy Int $v = Legacy::Util::number() + 1;\n", "a.tpr:2:", "operator `+` requires Int operands");
    rejects("use Legacy::Util;\nif (Legacy::Util::flag()) {\n}\n", "a.tpr:2:", "if condition must be Bool, found Any");
}

#[test]
fn rejects_interpolating_non_str() {
    rejects("my Int $n = 1;\nmy Str $s = \"n=$n\";\n", "a.tpr:2:", "only Str variables can be interpolated; `$n` is Int");
    rejects("sub f(Optional[Str] $o) -> Str {\n    return \"$o\";\n}\n", "a.tpr:2:", "`$o` is Optional[Str]");
}

#[test]
fn rejects_interpolating_undeclared() {
    rejects("my Str $s = \"$nope\";\n", "a.tpr:1:", "undeclared variable `$nope`");
}

#[test]
fn rejects_builtin_functions() {
    rejects("print(\"x\");\n", "a.tpr:1:", "unknown function `print`");
    rejects("my Int $n = length(\"x\");\n", "a.tpr:1:", "unknown function `length`");
}

#[test]
fn rejects_unqualified_call_to_legacy_function() {
    rejects("use Legacy::Util;\nemit(\"x\");\n", "a.tpr:2:", "unknown function `emit`");
}

#[test]
fn rejects_call_into_unused_package() {
    rejects("my Any $v = Legacy::Util::name();\n", "a.tpr:1:", "package `Legacy::Util` is not used");
}

#[test]
fn rejects_typed_module_call_bad_argument() {
    assert_err(
        &with(&[("Math.tpm", MATH)], "use Math;\nmy Int $z = Math::add(\"a\", 2);\n"),
        "a.tpr:2:",
        "argument 1 of Math::add: type mismatch: expected Int, found Str",
    );
}

#[test]
fn rejects_typed_module_call_arity() {
    assert_err(&with(&[("Math.tpm", MATH)], "use Math;\nmy Int $z = Math::add(1);\n"), "a.tpr:2:", "Math::add expects 2 arguments, found 1");
}

#[test]
fn rejects_unknown_function_in_typed_module() {
    assert_err(&with(&[("Math.tpm", MATH)], "use Math;\nmy Int $z = Math::nope(1);\n"), "a.tpr:2:", "unknown function `Math::nope`");
}

#[test]
fn rejects_named_function_arg_errors() {
    let cases = [
        ("area(w => 1, h => 2, d => 3)", "unknown named argument `d` for area"),
        ("area(w => 1)", "missing required named argument `h` for area"),
        ("area(w => \"1\", h => 2)", "named argument `w` of area: type mismatch: expected Int, found Str"),
        ("area(w => 1, w => 1, h => 2)", "duplicate named argument `w`"),
        ("area(1, 2)", "area takes named arguments"),
    ];
    for (call, needle) in cases {
        rejects(&format!("sub area(:Int $w, :Int $h) -> Int {{\n    return $w * $h;\n}}\nmy Int $a = {call};\n"), "a.tpr:4:", needle);
    }
}

#[test]
fn rejects_named_args_to_positional_function() {
    rejects("sub f(Int $a) -> Int {\n    return $a;\n}\nmy Int $v = f(a => 1);\n", "a.tpr:4:", "f takes positional arguments");
}

#[test]
fn rejects_constructor_called_as_function() {
    assert_err(
        &with(&[("Point.tpm", MINI_POINT)], "use Point;\nmy Any $p = Point::new(\"Point\", 1, 2);\n"),
        "a.tpr:2:",
        "Point::new is a constructor; call it as `Point->new(...)`",
    );
}

#[test]
fn rejects_method_called_as_function() {
    assert_err(&with(&[("Point.tpm", MINI_POINT)], "use Point;\nmy Any $v = Point::x(1);\n"), "a.tpr:2:", "Point::x is a method");
    let module = format!("{MINI_POINT}\nsub f(Point $p) -> Int {{\n    return x($p);\n}}\n");
    rejects_module("Point.tpm", &module, "Point.tpm:14:", "Point::x is a method");
}

#[test]
fn rejects_heterogeneous_literals() {
    rejects("my Any $xs = [1, \"a\"];\n", "a.tpr:1:", "array literal elements must all have the same type");
    rejects("my Any $h = { a => 1, b => \"x\" };\n", "a.tpr:1:", "hash literal values must all have the same type");
}

#[test]
fn rejects_empty_literal_without_type_context() {
    rejects("use Legacy::Util;\nLegacy::Util::emit([]);\n", "a.tpr:2:", "cannot infer the type of an empty literal");
}

#[test]
fn rejects_foreach_over_non_arrayref() {
    rejects("foreach my Int $x (1) {\n}\n", "a.tpr:1:", "foreach requires ArrayRef[T], found Int");
}

#[test]
fn rejects_foreach_element_type_mismatch() {
    rejects("my ArrayRef[Int] $xs = [1];\nforeach my Str $x ($xs) {\n}\n", "a.tpr:2:", "type mismatch: expected Str, found Int");
}

#[test]
fn rejects_to_conversion_arity() {
    rejects("my Str $s = to_str();\n", "a.tpr:1:", "to_str takes exactly one argument");
    rejects("my Str $s = to_str(1, 2);\n", "a.tpr:1:", "to_str takes exactly one argument");
}

#[test]
fn rejects_die_with_non_str() {
    rejects("die 1;\n", "a.tpr:1:", "die requires a Str message, found Int");
}

#[test]
fn rejects_bad_types_in_my() {
    rejects("my Foo $x = 1;\n", "a.tpr:1:", "unknown type `Foo`");
    rejects("my Class $c = 1;\n", "a.tpr:1:", "Class is only allowed");
    rejects("my Void $v = 1;\n", "a.tpr:1:", "Void is only allowed as a return type");
}

// ---- Task 3: structural subtyping ----

const SHAPE: &str = "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\n";
const CIRCLE: &str = "package Circle;\nfield r: Int;\nsub new(Class $class, :Int $r) -> Circle {\n    return bless({ r => $r }, $class);\n}\nsub area(Circle $self) -> Int {\n    return $self->{r} * $self->{r};\n}\n";

#[test]
fn accepts_class_satisfying_interface() {
    assert_ok(&with(
        &[("Shape.tpm", SHAPE), ("Circle.tpm", CIRCLE)],
        "use Shape;\nuse Circle;\nsub f(Shape $s) -> Int {\n    return $s->area;\n}\nmy Circle $c = Circle->new(r => 2);\nmy Int $a = f($c);\n",
    ));
}

#[test]
fn rejects_class_missing_interface_method() {
    assert_err(
        &with(
            &[("Shape.tpm", SHAPE), ("Nope.tpm", "package Nope;\nfield r: Int;\nsub new(Class $class, :Int $r) -> Nope {\n    return bless({ r => $r }, $class);\n}\n")],
            "use Shape;\nuse Nope;\nsub f(Shape $s) -> Int {\n    return 1;\n}\nmy Nope $n = Nope->new(r => 1);\nmy Int $a = f($n);\n",
        ),
        "a.tpr:",
        "type mismatch",
    );
}

#[test]
fn rejects_interface_signature_mismatch() {
    assert_err(
        &with(
            &[("Shape.tpm", SHAPE), ("Bad.tpm", "package Bad;\nfield r: Int;\nsub new(Class $class, :Int $r) -> Bad {\n    return bless({ r => $r }, $class);\n}\nsub area(Bad $self) -> Str {\n    return \"x\";\n}\n")],
            "use Shape;\nuse Bad;\nsub f(Shape $s) -> Int {\n    return 1;\n}\nmy Bad $b = Bad->new(r => 1);\nmy Int $a = f($b);\n",
        ),
        "a.tpr:",
        "type mismatch",
    );
}

#[test]
fn accepts_literals_under_optional_and_union_expectations() {
    assert_ok(&script("my Optional[ArrayRef[Str]] $t = [];\n"));
    assert_ok(&script("my Optional[ArrayRef[Int|Str]] $z = [1, \"a\"];\n"));
    assert_ok(&script("my ArrayRef[Int|Str]|Str $z = [1, \"a\"];\n"));
    assert_ok(&script("my Optional[HashRef[Str]] $h = {};\n"));
    assert_ok(&script("sub f(Optional[ArrayRef[Str]] $t) -> Int {\n    return 1;\n}\nmy Int $n = f([]);\n"));
}

#[test]
fn rejects_literal_element_mismatch_under_optional() {
    rejects("my Optional[ArrayRef[Int]] $t = [\"a\"];\n", "a.tpr:1:", "type mismatch: expected Int, found Str");
}

#[test]
fn rejects_empty_literal_for_ambiguous_union() {
    rejects("my ArrayRef[Int]|ArrayRef[Str] $u = [];\n", "a.tpr:1:", "cannot infer the type of an empty literal");
}

// ---- Task 12: literals under unions containing Any / Optional members ----

#[test]
fn any_in_union_leaves_array_literal_unconstrained() {
    assert_ok(&script("my Any|ArrayRef[Int] $v = [\"a\"];\n"));
    assert_ok(&script("my Optional[Any|ArrayRef[Int]] $w = [\"a\"];\n"));
    assert_ok(&script("my Optional[Any]|ArrayRef[Int] $x = [\"a\"];\n"));
}

#[test]
fn any_in_union_leaves_hash_literal_unconstrained() {
    assert_ok(&script("my Any|HashRef[Int] $v = { a => \"x\" };\n"));
}

#[test]
fn any_in_union_still_needs_context_for_empty_literal() {
    rejects("my Any|ArrayRef[Int] $v = [];\n", "a.tpr:1:", "cannot infer the type of an empty literal");
}

#[test]
fn union_without_any_still_checks_literal_elements() {
    rejects("my ArrayRef[Int]|Str $v = [1, \"a\"];\n", "a.tpr:1:", "type mismatch: expected Int, found Str");
}

#[test]
fn optional_container_inside_union_member_gives_literal_context() {
    assert_ok(&script("my Optional[ArrayRef[Str]]|Int $v = [];\n"));
}

#[test]
fn union_without_any_still_checks_hash_literal_values() {
    rejects("my HashRef[Int]|Str $v = { a => \"x\" };\n", "a.tpr:1:", "type mismatch: expected Int, found Str");
}
