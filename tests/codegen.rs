mod common;
use common::*;

#[test]
fn public_constructor_matches_spec_example() {
    let src = "package Point;\n\nfield x: Int;\nfield y: Int;\n\nsub new(Class $class, :Int $x, :Int $y) -> Point {\n    return bless({ x => $x, y => $y }, $class);\n}\n";
    let out = build(&[("Point.tpm", src)], &["Point.tpm"]);
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "Point.tpm"),
        r#"package Point;
use strict;
use warnings;
use Types::Standard qw(Int);

sub new {
    my ($class, %args) = @_;
    Int->assert_valid($args{x});
    Int->assert_valid($args{y});
    return bless({ x => $args{x}, y => $args{y} }, $class);
}

1;
"#
    );
}

#[test]
fn public_method_checks_exact_class_and_optional_uses_maybe() {
    let out = build(&[("Point.tpm", POINT)], &["Point.tpm"]);
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "Point.tpm"),
        r#"package Point;
use strict;
use warnings;
use Types::Standard qw(InstanceOf Int Maybe Str);

sub new {
    my ($class, %args) = @_;
    Int->assert_valid($args{x});
    Int->assert_valid($args{y});
    (Maybe[Str])->assert_valid($args{label});
    return bless({ x => $args{x}, y => $args{y}, label => $args{label} }, $class);
}

sub x {
    my ($self) = @_;
    (InstanceOf["Point"])->where(sub { ref($_) eq "Point" })->assert_valid($self);
    return $self->{x};
}

sub move {
    my ($self, %args) = @_;
    (InstanceOf["Point"])->where(sub { ref($_) eq "Point" })->assert_valid($self);
    Int->assert_valid($args{x});
    Int->assert_valid($args{y});
    return Point->new(x => $args{x}, y => $args{y}, label => $self->{label});
}

1;
"#
    );
}

#[test]
fn public_function_has_positional_checks() {
    let out = build(&[("Math.tpm", "package Math;\n\nsub add(Int $x, Int $y) -> Int {\n    return $x + $y;\n}\n")], &["Math.tpm"]);
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "Math.tpm"),
        r#"package Math;
use strict;
use warnings;
use Types::Standard qw(Int);

sub add {
    my ($x, $y) = @_;
    Int->assert_valid($x);
    Int->assert_valid($y);
    return $x + $y;
}

1;
"#
    );
}

#[test]
fn script_subs_have_no_entry_checks() {
    let out = script(
        "sub add(Int $x, Int $y) -> Int {\n    return $x + $y;\n}\n\nsub area(:Int $w, :Int $h) -> Int {\n    return $w * $h;\n}\n\nmy Int $z = add(1, 2);\nmy Int $a = area(w => 2, h => 3);\n",
    );
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;

sub add {
    my ($x, $y) = @_;
    return $x + $y;
}

sub area {
    my (%args) = @_;
    return $args{w} * $args{h};
}

my $z = add(1, 2);
my $a = area(w => 2, h => 3);
"#
    );
}

#[test]
fn uses_have_empty_import_lists() {
    let out = build(
        &[
            ("Math.tpm", "package Math;\n\nsub add(Int $x, Int $y) -> Int {\n    return $x + $y;\n}\n"),
            ("a.tpr", "use Math;\nuse Legacy::Util;\nmy Int $z = Math::add(1, 2);\nLegacy::Util::emit($z);\n"),
        ],
        &["a.tpr"],
    );
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;
use Math ();
use Legacy::Util ();

my $z = Math::add(1, 2);
Legacy::Util::emit($z);
"#
    );
}

#[test]
fn to_conversions_and_legacy_annotation_emit_runtime_checks() {
    let out = script(
        "use Legacy::Util;\nmy Str $s = to_str(Legacy::Util::name());\nmy Int $n = to_int(Legacy::Util::number());\nmy Bool $b = to_bool(Legacy::Util::flag());\nmy Legacy::Util $u = Legacy::Util->new();\nmy Legacy::Util $v = $u;\n",
    );
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;
use Types::Standard qw(Bool InstanceOf Int Str);
use Legacy::Util ();

my $s = Str->assert_return(Legacy::Util::name());
my $n = Int->assert_return(Legacy::Util::number());
my $b = Bool->assert_return(Legacy::Util::flag());
my $u = (InstanceOf["Legacy::Util"])->where(sub { ref($_) eq "Legacy::Util" })->assert_return(Legacy::Util->new());
my $v = $u;
"#
    );
}

#[test]
fn expressions_keep_perl_precedence() {
    let out = script(
        "my Int $a = (1 + 2) * 3;\nmy Int $b = 1 - (2 - 3);\nmy Int $c = 1 - 2 - 3;\nmy Int $d = - -1;\nmy Int $e = -(1 + 2);\nmy Bool $f = 1 + 2 == 3;\nmy Str $g = \"a\" . (\"b\" . \"c\");\n",
    );
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;

my $a = (1 + 2) * 3;
my $b = 1 - (2 - 3);
my $c = 1 - 2 - 3;
my $d = -(-1);
my $e = -(1 + 2);
my $f = 1 + 2 == 3;
my $g = "a" . ("b" . "c");
"#
    );
}

#[test]
fn named_param_interpolation_reads_args_hash() {
    let out = build(&[("Greeter.tpm", "package Greeter;\n\nsub greet(:Str $name) -> Str {\n    return \"hello, $name!\";\n}\n")], &["Greeter.tpm"]);
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "Greeter.tpm"),
        r#"package Greeter;
use strict;
use warnings;
use Types::Standard qw(Str);

sub greet {
    my (%args) = @_;
    Str->assert_valid($args{name});
    return "hello, $args{name}!";
}

1;
"#
    );
}

#[test]
fn control_flow_and_literals() {
    let out = script(r#"use Legacy::Util;
sub sign(Int $n) -> Str {
    if ($n == 0) {
        return 'zero';
    } elsif ($n == 1) {
        return "one\n";
    } else {
        die "bad";
    }
}
sub show(ArrayRef[Int] $xs) -> Void {
    foreach my Int $x ($xs) {
        Legacy::Util::emit($x);
    }
}
show([1, 2]);
my HashRef[Int] $h = { a => 1, b => 2 };
my ArrayRef[Int] $e = [];
my HashRef[Int] $eh = {};
"#);
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;
use Legacy::Util ();

sub sign {
    my ($n) = @_;
    if ($n == 0) {
        return 'zero';
    } elsif ($n == 1) {
        return "one\n";
    } else {
        die "bad";
    }
}

sub show {
    my ($xs) = @_;
    foreach my $x (@{$xs}) {
        Legacy::Util::emit($x);
    }
    return;
}

show([1, 2]);
my $h = { a => 1, b => 2 };
my $e = [];
my $eh = {};
"#
    );
}

#[test]
fn negated_call_is_parenthesized_so_perl_does_not_read_a_file_test() {
    let out = script("sub f(Int $x) -> Int {\n    return $x;\n}\n\nmy Int $y = -f(3);\n");
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "a.tpr"),
        r#"#!/usr/bin/env perl
use strict;
use warnings;

sub f {
    my ($x) = @_;
    return $x;
}

my $y = -(f(3));
"#
    );
}

#[test]
fn interface_module_emits_package_only() {
    let out = build(
        &[("Shape.tpm", "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\n")],
        &["Shape.tpm"],
    );
    assert_ok(&out);
    assert_eq!(
        read_output(&out, "Shape.tpm"),
        "package Shape;\nuse strict;\nuse warnings;\n\n1;\n"
    );
}

#[test]
fn interface_param_emits_has_methods_check() {
    let out = build(
        &[
            ("Shape.tpm", "package Shape;\ninterface {\n    sub area(Shape $self) -> Int;\n}\n"),
            (
                "Use.tpm",
                "package Use;\nuse Shape;\nsub f(Shape $s) -> Int {\n    return $s->area;\n}\n",
            ),
        ],
        &["Use.tpm"],
    );
    assert_ok(&out);
    let text = read_output(&out, "Use.tpm");
    assert!(text.contains("use Types::Standard qw(HasMethods);"), "{text}");
    assert!(text.contains("(HasMethods[\"area\"])->assert_valid($s);"), "{text}");
}

#[test]
fn negated_method_chain_is_parenthesized_so_perl_does_not_read_a_file_test() {
    let out = build(
        &[
            (
                "C.tpm",
                "package C;\n\nfield x: Int;\n\nsub new(Class $class, :Int $x) -> C {\n    return bless({ x => $x }, $class);\n}\n\nsub x(C $self) -> Int {\n    return $self->{x};\n}\n",
            ),
            (
                "a.tpr",
                "use C;\n\nsub f(Int $n) -> C {\n    return C->new(x => $n);\n}\n\nmy Int $a = -f(3)->x;\nmy Int $b = -C->new(x => 4)->x();\n",
            ),
        ],
        &["C.tpm", "a.tpr"],
    );
    assert_ok(&out);
    let got = read_output(&out, "a.tpr");
    assert!(got.contains("my $a = -(f(3)->x());"), "{got}");
    assert!(got.contains("my $b = -(C->new(x => 4)->x());"), "{got}");
}
