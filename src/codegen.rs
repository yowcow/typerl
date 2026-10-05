use crate::ast::*;
use crate::checker::Facts;
use std::collections::{BTreeSet, HashSet};

pub fn generate(file: &File, facts: &Facts) -> String {
    let mut g = Gen {
        facts,
        public: file.kind == FileKind::Module,
        imports: BTreeSet::new(),
        named: HashSet::new(),
        out: Vec::new(),
    };
    let mut prev_stmt = false;
    for item in &file.items {
        match item {
            Item::Field(_) => {}
            Item::Sub(s) => {
                g.out.push(String::new());
                g.sub(s);
                prev_stmt = false;
            }
            Item::Stmt(s) => {
                if !prev_stmt {
                    g.out.push(String::new());
                }
                g.stmt(s, 0);
                prev_stmt = true;
            }
        }
    }
    let mut lines = vec![match &file.package {
        Some((p, _)) => format!("package {p};"),
        None => "#!/usr/bin/env perl".to_string(),
    }];
    lines.push("use strict;".into());
    lines.push("use warnings;".into());
    if !g.imports.is_empty() {
        let names: Vec<&str> = g.imports.iter().copied().collect();
        lines.push(format!("use Types::Standard qw({});", names.join(" ")));
    }
    for u in &file.uses {
        lines.push(format!("use {} ();", u.name));
    }
    lines.extend(g.out);
    if file.kind == FileKind::Module {
        lines.push(String::new());
        lines.push("1;".into());
    }
    lines.join("\n") + "\n"
}

struct Gen<'a> {
    facts: &'a Facts,
    public: bool,
    imports: BTreeSet<&'static str>,
    /// Named parameters of the sub being emitted (read as `$args{name}`).
    named: HashSet<String>,
    out: Vec<String>,
}

impl Gen<'_> {
    fn line(&mut self, depth: usize, s: String) {
        self.out.push(format!("{}{s}", "    ".repeat(depth)));
    }

    fn imp(&mut self, name: &'static str) -> String {
        self.imports.insert(name);
        name.to_string()
    }

    /// Type::Tiny expression for a value type.
    fn tt(&mut self, t: &Type) -> String {
        match t {
            Type::Int => self.imp("Int"),
            Type::Str => self.imp("Str"),
            Type::Bool => self.imp("Bool"),
            Type::Any => self.imp("Any"),
            Type::ArrayRef(e) => {
                let i = self.tt(e);
                format!("{}[{i}]", self.imp("ArrayRef"))
            }
            Type::HashRef(e) => {
                let i = self.tt(e);
                format!("{}[{i}]", self.imp("HashRef"))
            }
            Type::Optional(e) => {
                let i = self.tt(e);
                format!("{}[{i}]", self.imp("Maybe"))
            }
            Type::Union(ts) => {
                let parts: Vec<String> = ts.iter().map(|t| self.tt(t)).collect();
                format!("({})", parts.join(" | "))
            }
            Type::Object(c) => {
                self.imp("InstanceOf");
                format!("(InstanceOf[\"{c}\"])->where(sub {{ ref($_) eq \"{c}\" }})")
            }
            Type::Void | Type::Class => unreachable!("the checker rejects Void/Class values"),
        }
    }

    /// `tt` wrapped so a following `->assert_*` applies to the whole type.
    fn check_target(&mut self, t: &Type) -> String {
        let s = self.tt(t);
        if matches!(t, Type::ArrayRef(_) | Type::HashRef(_) | Type::Optional(_)) {
            format!("({s})")
        } else {
            s
        }
    }

    fn sub(&mut self, s: &Sub) {
        self.named = s.params.iter().filter(|p| p.named).map(|p| p.name.clone()).collect();
        self.line(0, format!("sub {} {{", s.name));
        let mut vars: Vec<String> = s.params.iter().filter(|p| !p.named).map(|p| format!("${}", p.name)).collect();
        if !self.named.is_empty() {
            vars.push("%args".into());
        }
        if !vars.is_empty() {
            self.line(1, format!("my ({}) = @_;", vars.join(", ")));
        }
        if self.public {
            for p in &s.params {
                if matches!(p.ty, Type::Any | Type::Class) {
                    continue;
                }
                let v = if p.named { format!("$args{{{}}}", p.name) } else { format!("${}", p.name) };
                let t = self.check_target(&p.ty);
                self.line(1, format!("{t}->assert_valid({v});"));
            }
        }
        for st in &s.body {
            self.stmt(st, 1);
        }
        if s.ret == Type::Void && !terminates(&s.body) {
            self.line(1, "return;".into());
        }
        self.line(0, "}".into());
        self.named.clear();
    }

    fn stmt(&mut self, st: &Stmt, d: usize) {
        match st {
            Stmt::My { ty, name, init, span } => {
                let mut v = self.expr(init);
                if self.facts.narrow.contains(span) {
                    v = format!("{}->assert_return({v})", self.check_target(ty));
                }
                self.line(d, format!("my ${name} = {v};"));
            }
            Stmt::If { arms, els, .. } => {
                for (i, (c, b)) in arms.iter().enumerate() {
                    let c = self.expr(c);
                    let head = if i == 0 { format!("if ({c}) {{") } else { format!("}} elsif ({c}) {{") };
                    self.line(d, head);
                    for s in b {
                        self.stmt(s, d + 1);
                    }
                }
                if let Some(b) = els {
                    self.line(d, "} else {".into());
                    for s in b {
                        self.stmt(s, d + 1);
                    }
                }
                self.line(d, "}".into());
            }
            Stmt::Foreach { var, list, body, .. } => {
                let l = self.expr(list);
                self.line(d, format!("foreach my ${var} (@{{{l}}}) {{"));
                for s in body {
                    self.stmt(s, d + 1);
                }
                self.line(d, "}".into());
            }
            Stmt::Return { value: Some(v), .. } => {
                let v = self.expr(v);
                self.line(d, format!("return {v};"));
            }
            Stmt::Return { value: None, .. } => self.line(d, "return;".into()),
            Stmt::Die { msg, .. } => {
                let m = self.expr(msg);
                self.line(d, format!("die {m};"));
            }
            Stmt::Expr(e) => {
                let e = self.expr(e);
                self.line(d, format!("{e};"));
            }
        }
    }

    fn var(&self, n: &str) -> String {
        if self.named.contains(n) {
            format!("$args{{{n}}}")
        } else {
            format!("${n}")
        }
    }

    fn pairs(&mut self, pairs: &[Pair]) -> String {
        pairs.iter().map(|p| format!("{} => {}", p.key, self.expr(&p.value))).collect::<Vec<_>>().join(", ")
    }

    fn hash(&mut self, pairs: &[Pair]) -> String {
        if pairs.is_empty() {
            return "{}".into();
        }
        format!("{{ {} }}", self.pairs(pairs))
    }

    fn args(&mut self, a: &Args) -> String {
        match a {
            Args::Positional(v) => v.iter().map(|e| self.expr(e)).collect::<Vec<_>>().join(", "),
            Args::Named(p) => self.pairs(p),
        }
    }

    /// Operand of a binary operator: parenthesize lower precedence, and equal precedence on the right.
    fn operand(&mut self, e: &Expr, parent: u8, right: bool) -> String {
        let s = self.expr(e);
        match &e.kind {
            ExprKind::Binary(op, ..) if op.prec() < parent || (right && op.prec() == parent) => format!("({s})"),
            _ => s,
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => n.clone(),
            ExprKind::Str(StrLit::Single(raw)) => raw.clone(),
            ExprKind::Str(StrLit::Double(parts)) => {
                let mut s = String::from("\"");
                for p in parts {
                    match p {
                        StrPart::Lit(l) => s.push_str(l),
                        StrPart::Var(n, _) => s.push_str(&self.var(n)),
                    }
                }
                s.push('"');
                s
            }
            ExprKind::Var(n) => self.var(n),
            ExprKind::Array(items) => {
                let v: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                format!("[{}]", v.join(", "))
            }
            ExprKind::Hash(pairs) => self.hash(pairs),
            ExprKind::Neg(x) => {
                let s = self.expr(x);
                // `-f(3)` / `-C->new(..)->x` read as a file test in Perl for a single-letter root,
                // so everything except a plain int or variable is parenthesized.
                if matches!(x.kind, ExprKind::Int(_) | ExprKind::Var(_)) {
                    format!("-{s}")
                } else {
                    format!("-({s})")
                }
            }
            ExprKind::Binary(op, l, r) => {
                let ls = self.operand(l, op.prec(), false);
                let rs = self.operand(r, op.prec(), true);
                format!("{ls} {} {rs}", op.symbol())
            }
            ExprKind::Call { name, args } => {
                let conv = match name.as_str() {
                    "to_int" => Some("Int"),
                    "to_str" => Some("Str"),
                    "to_bool" => Some("Bool"),
                    _ => None,
                };
                let a = self.args(args);
                match conv {
                    Some(t) => format!("{}->assert_return({a})", self.imp(t)),
                    None => format!("{name}({a})"),
                }
            }
            ExprKind::ClassCall { class, method, args } => {
                let a = self.args(args);
                format!("{class}->{method}({a})")
            }
            ExprKind::MethodCall { recv, method, args } => {
                let r = self.expr(recv);
                let a = self.args(args);
                format!("{r}->{method}({a})")
            }
            ExprKind::Field { recv, name } => {
                let r = self.expr(recv);
                format!("{r}->{{{name}}}")
            }
            ExprKind::Bless { fields, target } => {
                let h = self.hash(fields);
                let t = self.expr(target);
                format!("bless({h}, {t})")
            }
        }
    }
}
