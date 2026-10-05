mod common;
use common::*;

// The spec's Point. Lines: 7 sub new, 8 bless, 11 sub x, 15 sub move, 16 Point->new.
const POINT: &str = r#"package Point;

field x: Int;
field y: Int;
field label: Optional[Str];

sub new(Class $class, :Int $x, :Int $y, :Optional[Str] $label) -> Point {
    return bless({ x => $x, y => $y, label => $label }, $class);
}

sub x(Point $self) -> Int {
    return $self->{x};
}

sub move(Point $self, :Int $x, :Int $y) -> Point {
    return Point->new(x => $x, y => $y, label => $self->{label});
}
"#;

const LABEL: &str = r#"package Point::Label;

field text: Str;

sub new(Class $class, :Str $text) -> Point::Label {
    return bless({ text => $text }, $class);
}

sub text(Point::Label $self) -> Str {
    return $self->{text};
}
"#;

const PIN: &str = r#"package Pin;
use Point;

field at: Point;

sub new(Class $class, :Point $at) -> Pin {
    return bless({ at => $at }, $class);
}

sub x(Pin $self) -> Int {
    return $self->{at}->x();
}
"#;

fn with_point(main: &str) -> Out {
    build(&[("Point.tpm", POINT), ("Point/Label.tpm", LABEL), ("a.tpr", main)], &["a.tpr"])
}

/// Point.tpm whose constructor body (line 5) is `body`.
fn ctor(body: &str) -> Out {
    let src = format!("package Point;\nfield x: Int;\nfield label: Optional[Str];\nsub new(Class $class, :Int $x, :Str $c) -> Point {{\n    {body}\n}}\n");
    build(&[("Point.tpm", src.as_str())], &["Point.tpm"])
}

/// Point.tpm with a method `get` whose body (line 7) is `body`.
fn getter(body: &str) -> Out {
    let src = format!("package Point;\nfield x: Int;\nsub new(Class $class, :Int $x) -> Point {{\n    return bless({{ x => $x }}, $class);\n}}\nsub get(Point $self) -> Int {{\n    {body}\n}}\n");
    build(&[("Point.tpm", src.as_str())], &["Point.tpm"])
}

// ---- accepted ----

#[test]
fn point_module_from_spec_typechecks() {
    assert_ok(&build(&[("Point.tpm", POINT)], &["Point.tpm"]));
}

#[test]
fn constructor_call_with_named_args() {
    assert_ok(&with_point("use Point;\nmy Point $p = Point->new(x => 1, y => 2);\nmy Int $x = $p->x();\n"));
}

#[test]
fn named_args_in_any_order() {
    assert_ok(&with_point(
        "use Point;\nmy Point $p = Point->new(y => 2, x => 1);\nmy Point $q = Point->new(y => 2, x => 1, label => \"origin\");\n",
    ));
}

#[test]
fn method_calls_with_and_without_parens_and_chaining() {
    assert_ok(&with_point(
        "use Point;\nmy Point $p = Point->new(x => 1, y => 2);\nmy Int $a = $p->x;\nmy Point $q = $p->move(y => 1, x => 2);\nmy Int $b = Point->new(x => 1, y => 2)->move(x => 3, y => 4)->x();\n",
    ));
}

#[test]
fn bless_matching_fields_accepted() {
    assert_ok(&ctor("return bless({ x => $x, label => $c }, $class);"));
}

#[test]
fn bless_may_omit_optional_field() {
    assert_ok(&ctor("return bless({ x => $x }, $class);"));
}

#[test]
fn field_read_in_same_package() {
    assert_ok(&getter("return $self->{x};"));
    assert_ok(&build(
        &[("Point.tpm", "package Point;\nfield x: Int;\nsub dx(Point $self, Point $b) -> Int {\n    return $self->{x} - $b->{x};\n}\n")],
        &["Point.tpm"],
    ));
}

#[test]
fn rejects_field_read_in_function() {
    assert_err(
        &build(&[("Point.tpm", "package Point;\nfield x: Int;\nsub dx(Point $a, Point $b) -> Int {\n    return $a->{x} - $b->{x};\n}\n")], &["Point.tpm"]),
        "Point.tpm:4:",
        "fields of Point can only be read in a method of Point",
    );
}

#[test]
fn delegation_field_accepted() {
    assert_ok(&build(&[("Point.tpm", POINT), ("Pin.tpm", PIN)], &["Pin.tpm"]));
}

#[test]
fn use_point_label_resolves_to_file() {
    assert_ok(&with_point(
        "use Point::Label;\nmy Point::Label $l = Point::Label->new(text => \"origin\");\nmy Str $t = $l->text();\n",
    ));
}

#[test]
fn point_label_signature_is_checked() {
    assert_err(
        &with_point("use Point::Label;\nmy Point::Label $l = Point::Label->new(text => 1);\n"),
        "a.tpr:2:",
        "named argument `text` of Point::Label::new: type mismatch: expected Str, found Int",
    );
}

#[test]
fn point_label_without_file_is_legacy_perl() {
    assert_ok(&script("use Point::Label;\nmy Any $l = Point::Label->new(text => 1);\n"));
}

#[test]
fn legacy_class_annotation_narrows_any() {
    assert_ok(&script(
        "use Legacy::Util;\nmy Legacy::Util $u = Legacy::Util->new();\nmy Legacy::Util $v = $u;\nmy Any $n = $u->name();\n",
    ));
}

// ---- rejected: bless ----

#[test]
fn rejects_bless_with_string_target() {
    assert_err(&ctor("return bless({ x => $x }, \"Point\");"), "Point.tpm:5:", "bless target must be the constructor's `$class`");
}

#[test]
fn rejects_bless_with_arbitrary_variable() {
    assert_err(&ctor("return bless({ x => $x }, $c);"), "Point.tpm:5:", "bless target must be the constructor's `$class`");
}

#[test]
fn rejects_bless_undeclared_key() {
    assert_err(&ctor("return bless({ x => $x, z => 1 }, $class);"), "Point.tpm:5:", "unknown field `z` in bless for Point");
}

#[test]
fn rejects_bless_missing_required_field() {
    assert_err(&ctor("return bless({ label => $c }, $class);"), "Point.tpm:5:", "missing field `x` in bless for Point");
}

#[test]
fn rejects_bless_field_type_mismatch() {
    assert_err(&ctor("return bless({ x => $c }, $class);"), "Point.tpm:5:", "field `x` of Point: type mismatch: expected Int, found Str");
}

#[test]
fn rejects_bless_duplicate_key() {
    assert_err(&ctor("return bless({ x => $x, x => $x }, $class);"), "Point.tpm:5:", "duplicate key `x`");
}

#[test]
fn rejects_bless_not_directly_returned() {
    assert_err(
        &ctor("my Point $p = bless({ x => $x }, $class);\n    return $p;"),
        "Point.tpm:5:",
        "bless must appear directly in `return`",
    );
}

#[test]
fn rejects_class_value_misuse() {
    assert_err(&ctor("my Any $k = $class;\n    return bless({ x => $x }, $class);"), "Point.tpm:5:", "found Class");
}

#[test]
fn rejects_constructor_without_class_param() {
    let point = "package Point;\nfield x: Int;\nsub new(:Int $x) -> Point {\n    return bless({ x => $x }, $class);\n}\n";
    assert_err(&build(&[("Point.tpm", point)], &["Point.tpm"]), "Point.tpm:4:", "bless is only allowed in a constructor");
    assert_err(
        &build(&[("Point.tpm", point), ("a.tpr", "use Point;\nmy Point $p = Point->new(x => 1);\n")], &["a.tpr"]),
        "a.tpr:2:",
        "Point::new is not a constructor",
    );
}

// ---- rejected: constructor calls / named args ----

#[test]
fn rejects_named_unknown_key() {
    assert_err(&with_point("use Point;\nmy Point $p = Point->new(x => 1, y => 2, z => 3);\n"), "a.tpr:2:", "unknown named argument `z` for Point::new");
}

#[test]
fn rejects_named_missing_required() {
    assert_err(&with_point("use Point;\nmy Point $p = Point->new(x => 1);\n"), "a.tpr:2:", "missing required named argument `y` for Point::new");
}

#[test]
fn rejects_named_type_mismatch() {
    assert_err(
        &with_point("use Point;\nmy Point $p = Point->new(x => \"1\", y => 2);\n"),
        "a.tpr:2:",
        "named argument `x` of Point::new: type mismatch: expected Int, found Str",
    );
}

#[test]
fn rejects_named_duplicate_key() {
    assert_err(&with_point("use Point;\nmy Point $p = Point->new(x => 1, x => 1, y => 2);\n"), "a.tpr:2:", "duplicate named argument `x`");
}

#[test]
fn rejects_positional_constructor_call() {
    assert_err(&with_point("use Point;\nmy Point $p = Point->new(1, 2);\n"), "a.tpr:2:", "Point::new takes named arguments");
}

#[test]
fn rejects_class_passed_as_named_arg() {
    assert_err(
        &with_point("use Point;\nmy Point $p = Point->new(class => \"Point\", x => 1, y => 2);\n"),
        "a.tpr:2:",
        "unknown named argument `class` for Point::new",
    );
}

// ---- rejected: fields ----

#[test]
fn rejects_undeclared_field_read() {
    assert_err(&getter("return $self->{z};"), "Point.tpm:7:", "unknown field `z` on Point");
}

#[test]
fn rejects_field_read_outside_package() {
    assert_err(
        &with_point("use Point;\nmy Point $p = Point->new(x => 1, y => 2);\nmy Int $x = $p->{x};\n"),
        "a.tpr:3:",
        "fields of Point are private to package Point",
    );
}

#[test]
fn rejects_field_read_on_legacy_object() {
    rejects(
        "use Legacy::Util;\nmy Legacy::Util $u = Legacy::Util->new();\nmy Any $v = $u->{k};\n",
        "a.tpr:3:",
        "fields of Legacy::Util are private to package Legacy::Util",
    );
}

#[test]
fn rejects_hashref_element_access() {
    rejects("my HashRef[Int] $h = { a => 1 };\nmy Int $v = $h->{a};\n", "a.tpr:2:", "element access on HashRef is not supported");
}

// ---- rejected: methods ----

#[test]
fn rejects_unknown_method() {
    assert_err(&with_point("use Point;\nmy Point $p = Point->new(x => 1, y => 2);\nmy Int $v = $p->nope();\n"), "a.tpr:3:", "Point has no method `nope`");
}

#[test]
fn rejects_constructor_called_on_instance() {
    assert_err(
        &with_point("use Point;\nmy Point $p = Point->new(x => 1, y => 2);\nmy Point $q = $p->new(x => 1, y => 2);\n"),
        "a.tpr:3:",
        "constructor `new` must be called on the class",
    );
}

#[test]
fn rejects_method_called_on_class() {
    assert_err(&with_point("use Point;\nmy Int $v = Point->x();\n"), "a.tpr:2:", "Point::x is not a constructor");
}

#[test]
fn rejects_function_called_as_method() {
    let point = "package Point;\nfield x: Int;\nsub new(Class $class, :Int $x) -> Point {\n    return bless({ x => $x }, $class);\n}\nsub origin() -> Point {\n    return Point->new(x => 0);\n}\n";
    assert_err(
        &build(&[("Point.tpm", point), ("a.tpr", "use Point;\nmy Point $p = Point::origin();\nmy Point $q = $p->origin();\n")], &["a.tpr"]),
        "a.tpr:3:",
        "`origin` is a function, not a method",
    );
}

#[test]
fn rejects_other_class_where_class_expected() {
    assert_err(
        &with_point("use Point;\nuse Point::Label;\nsub f(Point $p) -> Int {\n    return 1;\n}\nmy Int $v = f(Point::Label->new(text => \"a\"));\n"),
        "a.tpr:6:",
        "type mismatch: expected Point, found Point::Label",
    );
}

#[test]
fn rejects_method_call_on_any() {
    rejects("use Legacy::Util;\nmy Any $v = Legacy::Util::name();\nmy Any $w = $v->foo();\n", "a.tpr:3:", "cannot call a method on Any");
}

#[test]
fn rejects_method_call_on_primitive() {
    rejects("my Int $n = 1;\nmy Any $v = $n->foo();\n", "a.tpr:2:", "cannot call a method on Int");
}

#[test]
fn rejects_any_narrowed_to_typed_class() {
    assert_err(
        &with_point("use Point;\nuse Legacy::Util;\nmy Point $p = Legacy::Util::name();\n"),
        "a.tpr:3:",
        "type mismatch: expected Point, found Any",
    );
}

#[test]
fn rejects_legacy_method_result_as_str() {
    rejects(
        "use Legacy::Util;\nmy Legacy::Util $u = Legacy::Util->new();\nmy Str $s = $u->name();\n",
        "a.tpr:3:",
        "type mismatch: expected Str, found Any",
    );
}

#[test]
fn rejects_class_call_on_unused_package() {
    rejects("my Any $v = Foo->new();\n", "a.tpr:1:", "package `Foo` is not used");
}
