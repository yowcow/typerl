mod common;
use common::*;

// ---- accepted grammar ----

#[test]
fn accepts_full_script_grammar() {
    assert_ok(&script(
        r#"use Legacy::Util;

sub add(Int $x, Int $y) -> Int {
    return $x + $y;
}

sub greet(:Str $name, :Optional[Str] $title) -> Str {
    return "hello, $name";
}

sub sign(Int $n) -> Str {
    if ($n == 0) {
        return 'zero';
    } elsif ($n == -1 * -1) {
        return "one";
    } else {
        die "unexpected";
    }
}

my Int $sum = add(1, 2) * (3 - -4);
my Str $s = greet(title => "Dr.", name => "x") . sign(0);
my ArrayRef[Int] $xs = [1, 2, 3];
my HashRef[Str] $h = { a => "x", b => "y" };
my ArrayRef[Int] $empty = [];
my Bool $same = "a" eq "a";
foreach my Int $x ($xs) {
    Legacy::Util::emit($x);
}
my Any $r = Legacy::Util->new();
my Int $n = to_int(Legacy::Util::number());
Legacy::Util::emit("sum=\$ $s \@ \"q\"\n\t\\");
"#,
    ));
}

#[test]
fn accepts_full_module_grammar() {
    let src = r#"package Point;
use Legacy::Util;

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

sub origin() -> Point {
    return Point->new(x => 0, y => 0);
}

sub norm1(Point $self) -> Int {
    return $self->x() + $self->{y};
}

sub log_it(Point $self) -> Void {
    Legacy::Util::emit("point");
}
"#;
    assert_ok(&build(&[("Point.tpm", src)], &["Point.tpm"]));
}

#[test]
fn accepts_interface_block_with_bodyless_subs() {
    assert_ok(&build(
        &[(
            "Shape.tpm",
            "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\n",
        )],
        &["Shape.tpm"],
    ));
}

#[test]
fn rejects_bodyless_sub_outside_interface() {
    rejects_module(
        "Point.tpm",
        "package Point;\nsub area(Point $self) -> Int;\n",
        "Point.tpm:",
        "expected `{`",
    );
}

// ---- original spec 落とす list ----

#[test]
fn rejects_string_eval() {
    rejects(
        "my Str $code = \"1\";\neval $code;\n",
        "a.tpr:2:",
        "eval is not supported",
    );
    rejects(
        "my Any $v = eval(\"1\");\n",
        "a.tpr:1:",
        "eval is not supported",
    );
}

#[test]
fn rejects_reader_attribute() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int :reader;\n",
        "Point.tpm:2:",
        ":reader and :writer are not supported",
    );
}

#[test]
fn rejects_writer_attribute() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int :writer;\n",
        "Point.tpm:2:",
        ":reader and :writer are not supported",
    );
}

#[test]
fn rejects_my_without_type() {
    rejects(
        "my $x = 1;\n",
        "a.tpr:1:",
        "missing type annotation for `$x`",
    );
}

#[test]
fn rejects_param_without_type() {
    rejects(
        "sub f($x) -> Int {\n    return 1;\n}\n",
        "a.tpr:1:",
        "missing type annotation for `$x`",
    );
}

#[test]
fn rejects_sub_without_return_type() {
    rejects(
        "sub f(Int $x) {\n    return $x;\n}\n",
        "a.tpr:1:",
        "missing return type annotation",
    );
}

#[test]
fn rejects_field_without_type() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x;\n",
        "Point.tpm:2:",
        "missing type annotation for field `x`",
    );
}

#[test]
fn rejects_foreach_var_without_type() {
    rejects(
        "foreach my $x ([1]) {\n}\n",
        "a.tpr:1:",
        "missing type annotation for `$x`",
    );
}

#[test]
fn rejects_module_without_package() {
    rejects_module(
        "Point.tpm",
        "sub f() -> Int {\n    return 1;\n}\n",
        "Point.tpm:1:",
        "a module must start with `package Name;`",
    );
    rejects_module(
        "Point.tpm",
        "",
        "Point.tpm:1:",
        "a module must start with `package Name;`",
    );
}

#[test]
fn rejects_module_with_two_packages() {
    rejects_module(
        "Point.tpm",
        "package Point;\npackage Other;\n",
        "Point.tpm:2:",
        "only one package per module is allowed",
    );
}

#[test]
fn rejects_package_switch_mid_module() {
    rejects_module(
        "Point.tpm",
        "package Point;\nsub f() -> Int {\n    return 1;\n}\npackage Point;\n",
        "Point.tpm:5:",
        "only one package per module is allowed",
    );
}

#[test]
fn rejects_package_in_script() {
    rejects(
        "package Foo;\n",
        "a.tpr:1:",
        "package is not allowed in a script (.tpr)",
    );
    rejects(
        "my Int $x = 1;\npackage Foo;\n",
        "a.tpr:2:",
        "package is not allowed in a script (.tpr)",
    );
}

#[test]
fn rejects_require() {
    rejects("require Foo;\n", "a.tpr:1:", "require is not supported");
    rejects(
        "sub f() -> Int {\n    require Foo;\n    return 1;\n}\n",
        "a.tpr:2:",
        "require is not supported",
    );
}

#[test]
fn rejects_non_literal_use() {
    rejects(
        "use $m;\n",
        "a.tpr:1:",
        "use requires a literal module name",
    );
    rejects(
        "use \"Foo\";\n",
        "a.tpr:1:",
        "use requires a literal module name",
    );
}

#[test]
fn rejects_use_inside_sub() {
    rejects(
        "sub f() -> Int {\n    use Foo;\n    return 1;\n}\n",
        "a.tpr:2:",
        "use must appear at the top of the file",
    );
}

#[test]
fn rejects_sub_reference() {
    rejects(
        "sub f() -> Int {\n    return 1;\n}\nmy Any $r = \\&f;\n",
        "a.tpr:4:",
        "references (`\\`) are not supported",
    );
}

#[test]
fn rejects_ampersand_sigil() {
    rejects(
        "&f();\n",
        "a.tpr:1:",
        "`&` (subroutine sigil) is not supported",
    );
}

#[test]
fn rejects_anonymous_sub() {
    rejects(
        "my Any $f = sub (Int $x) -> Int {\n    return $x;\n};\n",
        "a.tpr:1:",
        "anonymous subs and closures are not supported",
    );
    rejects(
        "sub {\n};\n",
        "a.tpr:1:",
        "anonymous subs and closures are not supported",
    );
}

#[test]
fn rejects_nested_sub() {
    rejects(
        "sub f() -> Int {\n    sub g() -> Int {\n        return 1;\n    }\n    return 1;\n}\n",
        "a.tpr:2:",
        "nested subs are not supported",
    );
}

#[test]
fn rejects_list_assignment() {
    rejects(
        "my ($a, $b) = (1, 2);\n",
        "a.tpr:1:",
        "list assignment is not supported",
    );
}

#[test]
fn rejects_list_expression() {
    rejects(
        "sub f() -> Int {\n    return (1, 2);\n}\n",
        "a.tpr:2:",
        "list expressions (comma operator) are not supported",
    );
}

#[test]
fn rejects_wantarray() {
    rejects(
        "sub f() -> Int {\n    return wantarray();\n}\n",
        "a.tpr:2:",
        "wantarray is not supported",
    );
}

#[test]
fn rejects_code_ref_call() {
    rejects(
        "my Any $f = 1;\n$f->(1);\n",
        "a.tpr:2:",
        "calling code references is not supported",
    );
}

#[test]
fn rejects_rebless() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nsub new(Class $class, :Int $x) -> Point {\n    return bless({ x => $x }, $class);\n}\nsub again(Point $self) -> Point {\n    return bless($self, \"Point\");\n}\n",
        "Point.tpm:7:",
        "re-blessing is not allowed",
    );
}

#[test]
fn rejects_bless_with_one_argument() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nsub new(Class $class, :Int $x) -> Point {\n    return bless({ x => $x });\n}\n",
        "Point.tpm:4:",
        "bless requires two arguments",
    );
}

#[test]
fn rejects_field_assignment_outside_constructor() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nsub set_x(Point $self, Int $v) -> Point {\n    $self->{x} = $v;\n    return $self;\n}\n",
        "Point.tpm:4:",
        "field assignment is only allowed in the constructor's bless hash literal",
    );
}

#[test]
fn rejects_undeclared_field_write() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nsub set_z(Point $self, Int $v) -> Point {\n    $self->{z} = $v;\n    return $self;\n}\n",
        "Point.tpm:4:",
        "field assignment is only allowed in the constructor's bless hash literal",
    );
}

// ---- original spec 禁止 list / design decisions ----

#[test]
fn rejects_typeglob() {
    rejects("*foo = 1;\n", "a.tpr:1:", "typeglobs are not supported");
}

#[test]
fn rejects_symbolic_references() {
    rejects(
        "my Str $n = \"x\";\nmy Any $v = $$n;\n",
        "a.tpr:2:",
        "symbolic references and dereferencing are not supported",
    );
    rejects(
        "my Any $v = ${\"x\"};\n",
        "a.tpr:1:",
        "symbolic references and dereferencing are not supported",
    );
}

#[test]
fn rejects_phase_blocks() {
    for kw in ["BEGIN", "CHECK", "INIT", "END"] {
        rejects(
            &format!("{kw} {{\n}}\n"),
            "a.tpr:1:",
            "BEGIN/CHECK/INIT/END blocks are not supported",
        );
    }
    rejects_module(
        "Point.tpm",
        "package Point;\nBEGIN {\n}\n",
        "Point.tpm:2:",
        "BEGIN/CHECK/INIT/END blocks are not supported",
    );
}

#[test]
fn rejects_tie() {
    rejects("tie($x, \"Foo\");\n", "a.tpr:1:", "tie is not supported");
}

#[test]
fn rejects_local_and_our() {
    rejects(
        "our Int $x = 1;\n",
        "a.tpr:1:",
        "`our` declarations are not supported",
    );
    rejects(
        "local Int $x = 1;\n",
        "a.tpr:1:",
        "`local` declarations are not supported",
    );
}

#[test]
fn rejects_use_before_package() {
    rejects_module(
        "Point.tpm",
        "use Foo;\npackage Point;\n",
        "Point.tpm:1:",
        "a module must start with `package Name;`",
    );
}

#[test]
fn rejects_use_after_declarations() {
    rejects_module(
        "Point.tpm",
        "package Point;\nfield x: Int;\nuse Foo;\n",
        "Point.tpm:3:",
        "use must appear at the top of the file",
    );
    rejects(
        "my Int $x = 1;\nuse Foo;\n",
        "a.tpr:2:",
        "use must appear at the top of the file",
    );
}

#[test]
fn rejects_toplevel_statement_in_module() {
    rejects_module(
        "Point.tpm",
        "package Point;\nmy Int $x = 1;\n",
        "Point.tpm:2:",
        "module top level may only contain field and sub declarations",
    );
}

#[test]
fn rejects_field_in_script() {
    rejects(
        "field x: Int;\n",
        "a.tpr:1:",
        "field is only allowed at the top level of a module (.tpm)",
    );
}

#[test]
fn rejects_import_lists() {
    for src in ["use Foo qw(bar);\n", "use Foo 'bar';\n", "use Foo ();\n"] {
        rejects(src, "a.tpr:1:", "import lists are not supported");
    }
}

#[test]
fn rejects_use_strict_and_warnings() {
    rejects("use strict;\n", "a.tpr:1:", "emitted automatically");
    rejects("use warnings;\n", "a.tpr:1:", "emitted automatically");
}

#[test]
fn rejects_use_parent_and_base() {
    for src in [
        "use parent;\n",
        "use parent -norequire, 'Foo';\n",
        "use base 'Foo';\n",
    ] {
        rejects(src, "a.tpr:1:", "inheritance is not supported");
    }
}

#[test]
fn rejects_other_pragmas() {
    rejects(
        "use utf8;\n",
        "a.tpr:1:",
        "pragmas are not supported (`use utf8`)",
    );
    rejects(
        "use lib;\n",
        "a.tpr:1:",
        "pragmas are not supported (`use lib`)",
    );
}

#[test]
fn rejects_array_and_hash_variables() {
    rejects(
        "my Any $a = @xs;\n",
        "a.tpr:1:",
        "array variables are not supported (use ArrayRef)",
    );
    rejects(
        "my Any $h = %h;\n",
        "a.tpr:1:",
        "hash variables are not supported (use HashRef)",
    );
}

#[test]
fn rejects_reassignment() {
    rejects(
        "my Int $x = 1;\n$x = 2;\n",
        "a.tpr:2:",
        "reassignment is not supported",
    );
}

#[test]
fn rejects_array_element_access() {
    rejects(
        "my ArrayRef[Int] $xs = [1];\nmy Int $x = $xs->[0];\n",
        "a.tpr:2:",
        "array element access (`->[...]`) is not supported",
    );
}

#[test]
fn rejects_unsupported_operators() {
    let cases = [
        ("my Any $v = 1 / 2;\n", "/"),
        ("my Any $v = 1 % 2;\n", "%"),
        ("my Any $v = 1 < 2;\n", "<"),
        ("my Any $v = 1 > 2;\n", ">"),
        ("my Any $v = 1 != 2;\n", "!="),
        ("my Any $v = 1 <= 2;\n", "<="),
        ("my Any $v = 1 >= 2;\n", ">="),
        ("my Any $v = 1 <=> 2;\n", "<=>"),
        ("my Any $v = 1 && 2;\n", "&&"),
        ("my Any $v = 1 || 2;\n", "||"),
        ("my Any $v = 1 // 2;\n", "//"),
        ("my Any $v = !1;\n", "!"),
        ("my Any $v = ~1;\n", "~"),
        ("my Any $v = 2 ** 3;\n", "**"),
        ("my Any $v = 1 .. 2;\n", ".."),
        ("my Any $v = 1 ? 2 : 3;\n", "?"),
        ("my Any $v = \"a\" =~ \"b\";\n", "=~"),
        ("my Any $v = \"a\" x 3;\n", "x"),
        ("my Any $v = \"a\" ne \"b\";\n", "ne"),
        ("my Any $v = \"a\" lt \"b\";\n", "lt"),
        ("my Any $v = 1 and 2;\n", "and"),
        ("my Any $v = not 1;\n", "not"),
        ("my Int $x = 1;\n$x += 1;\n", "+="),
        ("my Int $x = 1;\n$x -= 1;\n", "-="),
        ("my Int $x = 1;\n$x *= 2;\n", "*="),
        ("my Str $x = \"a\";\n$x .= \"b\";\n", ".="),
        ("my Int $x = 1;\n$x++;\n", "++"),
        ("my Int $x = 1;\n--$x;\n", "--"),
    ];
    for (src, op) in cases {
        rejects(src, "a.tpr:", &format!("operator `{op}` is not supported"));
    }
}

#[test]
fn rejects_chained_comparison() {
    rejects(
        "my Bool $b = 1 == 1 == 1;\n",
        "a.tpr:1:",
        "chained comparisons are not supported",
    );
}

#[test]
fn rejects_bool_literals() {
    rejects(
        "my Bool $b = true;\n",
        "a.tpr:1:",
        "bareword `true` is not supported",
    );
    rejects(
        "my Bool $b = false;\n",
        "a.tpr:1:",
        "bareword `false` is not supported",
    );
}

#[test]
fn rejects_undef() {
    rejects(
        "my Optional[Int] $x = undef;\n",
        "a.tpr:1:",
        "undef is not supported",
    );
}

#[test]
fn rejects_statement_modifiers() {
    rejects(
        "die \"x\" if 1 == 1;\n",
        "a.tpr:1:",
        "statement modifiers are not supported",
    );
    rejects(
        "my Int $x = 1 unless 1 == 2;\n",
        "a.tpr:1:",
        "statement modifiers are not supported",
    );
}

#[test]
fn rejects_unsupported_control_flow() {
    rejects(
        "unless (1 == 1) {\n}\n",
        "a.tpr:1:",
        "`unless` is not supported",
    );
    rejects(
        "while (1 == 1) {\n}\n",
        "a.tpr:1:",
        "`while` is not supported",
    );
    rejects(
        "for my Int $i ([1]) {\n}\n",
        "a.tpr:1:",
        "`for` is not supported",
    );
    rejects("do {\n};\n", "a.tpr:1:", "`do` is not supported");
    rejects(
        "sub f() -> Void {\n    last;\n}\n",
        "a.tpr:2:",
        "`last` is not supported",
    );
}

#[test]
fn rejects_non_call_expression_statement() {
    rejects(
        "1 + 2;\n",
        "a.tpr:1:",
        "only calls can be used as statements",
    );
    rejects(
        "\"a\";\n",
        "a.tpr:1:",
        "only calls can be used as statements",
    );
}

#[test]
fn rejects_bare_block() {
    rejects("{\n}\n", "a.tpr:1:", "bare blocks are not supported");
}

#[test]
fn rejects_dynamic_method_name() {
    rejects(
        "my Any $m = 1;\nmy Any $o = 1;\n$o->$m();\n",
        "a.tpr:3:",
        "dynamic method names are not supported",
    );
}

#[test]
fn rejects_quote_like_operators() {
    rejects(
        "my Any $l = qw(a b);\n",
        "a.tpr:1:",
        "quote-like operators are not supported",
    );
    rejects(
        "my Any $l = qq(a);\n",
        "a.tpr:1:",
        "quote-like operators are not supported",
    );
}

#[test]
fn rejects_bad_number_literals() {
    rejects(
        "my Int $x = 010;\n",
        "a.tpr:1:",
        "leading zeros are not supported",
    );
    rejects(
        "my Int $x = 1.5;\n",
        "a.tpr:1:",
        "floating-point numbers are not supported",
    );
    rejects("my Int $x = 0x1F;\n", "a.tpr:1:", "invalid number literal");
    rejects("my Int $x = 1_000;\n", "a.tpr:1:", "invalid number literal");
}

#[test]
fn rejects_pod() {
    rejects(
        "=pod\n\nhi\n\n=cut\nmy Int $x = 1;\n",
        "a.tpr:1:1:",
        "POD is not supported",
    );
}

#[test]
fn rejects_backticks() {
    rejects(
        "my Any $v = `ls`;\n",
        "a.tpr:1:",
        "backticks are not supported",
    );
}

#[test]
fn rejects_special_variables() {
    for v in ["$_", "$0", "$@"] {
        rejects(
            &format!("my Any $v = {v};\n"),
            "a.tpr:1:",
            "special variables are not supported",
        );
    }
}

#[test]
fn rejects_package_variables() {
    rejects(
        "my Any $v = $Foo::x;\n",
        "a.tpr:1:",
        "package variables are not supported",
    );
}

#[test]
fn rejects_old_package_separator() {
    rejects(
        "my Any $v = Foo'bar();\n",
        "a.tpr:1:",
        "`'` as a package separator is not supported",
    );
}

#[test]
fn rejects_complex_interpolation() {
    for s in [
        "\"${s}\"",
        "\"$s->{k}\"",
        "\"$s->[0]\"",
        "\"$s[0]\"",
        "\"$s{k}\"",
        "\"$s::x\"",
        "\"$s's\"",
    ] {
        rejects(
            &format!("my Str $s = \"a\";\nmy Str $t = {s};\n"),
            "a.tpr:2:",
            "only simple `$name` interpolation of Str variables is supported",
        );
    }
    rejects(
        "my Str $t = \"@s\";\n",
        "a.tpr:1:",
        "`@` must be escaped as `\\@`",
    );
    rejects(
        "my Str $t = \"cost $5\";\n",
        "a.tpr:1:",
        "a literal `$` must be escaped as `\\$`",
    );
    rejects(
        "my Str $t = \"\\q\";\n",
        "a.tpr:1:",
        "unsupported escape `\\q`",
    );
}

#[test]
fn rejects_non_bareword_hash_key() {
    rejects(
        "my HashRef[Int] $h = { \"a\" => 1 };\n",
        "a.tpr:1:",
        "hash keys must be barewords",
    );
}

#[test]
fn rejects_qualified_sub_name() {
    rejects(
        "sub Foo::f() -> Int {\n    return 1;\n}\n",
        "a.tpr:1:",
        "sub names must not be qualified",
    );
}

#[test]
fn rejects_uninitialized_my() {
    rejects("my Int $x;\n", "a.tpr:1:", "variables must be initialized");
}

#[test]
fn rejects_die_in_expression() {
    rejects(
        "my Any $v = die(\"x\");\n",
        "a.tpr:1:",
        "die can only be used as a statement",
    );
}

#[test]
fn rejects_mixed_call_arguments() {
    rejects(
        "sub f(Int $a) -> Int {\n    return $a;\n}\nmy Int $v = f(1, b => 2);\n",
        "a.tpr:4:",
        "cannot mix positional and named arguments",
    );
    rejects(
        "sub f(Int $a) -> Int {\n    return $a;\n}\nmy Int $v = f(b => 2, 1);\n",
        "a.tpr:4:",
        "cannot mix positional and named arguments",
    );
}

// ---- resource limits: hostile input must give a diagnostic, never a stack overflow ----

#[test]
fn rejects_deeply_nested_input() {
    let n = 10_000;
    for src in [
        format!("my Int $v = {}1{};\n", "(".repeat(n), ")".repeat(n)),
        format!("my Int $v = {}", "(".repeat(n)),
        format!("my Any $v = {}{};\n", "[".repeat(n), "]".repeat(n)),
        format!("my Any $v = {}1{};\n", "f(".repeat(n), ")".repeat(n)),
        format!("my Any $v = {}1{};\n", "{ a => ".repeat(n), " }".repeat(n)),
        format!("my {}Int{} $v = 1;\n", "ArrayRef[".repeat(n), "]".repeat(n)),
        format!("{}{}", "if (1) {\n".repeat(n), "}\n".repeat(n)),
        format!("my Int $v = {}1;\n", "- ".repeat(n)),
    ] {
        rejects(&src, "a.tpr:", "nesting is too deep (limit 64)");
    }
}

#[test]
fn rejects_overlong_operator_chains() {
    let n = 10_000;
    rejects(
        &format!("my Int $v = 1{};\n", " + 1".repeat(n)),
        "a.tpr:",
        "expression is too long (limit 256)",
    );
    rejects(
        &format!("my Int $v = 1{};\n", " * 1".repeat(n)),
        "a.tpr:",
        "expression is too long (limit 256)",
    );
    rejects(
        &format!("my Any $v = Foo->new(){};\n", "->f".repeat(n)),
        "a.tpr:",
        "expression is too long (limit 256)",
    );
}

#[test]
fn accepts_input_at_the_limits() {
    assert_ok(&script(&format!(
        "my Int $v = {}1{};\n",
        "(".repeat(63),
        ")".repeat(63)
    )));
    assert_ok(&script(&format!("my Int $v = 1{};\n", " + 1".repeat(256))));
}

#[test]
fn rejects_non_bareword_field_key() {
    rejects(
        "my Any $v = $x->{\"a\"};\n",
        "a.tpr:1:18:",
        "field keys must be barewords",
    );
    rejects(
        "my Any $v = $x->{A::b};\n",
        "a.tpr:1:18:",
        "field keys must be barewords",
    );
}
