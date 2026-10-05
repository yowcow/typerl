use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::modules::{self, Loader, ModuleSig, SubKind, SubSig};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Facts {
    pub narrow: HashSet<Span>,
}

type R<T> = Result<T, Diag>;

fn signatures_equal(a: &SubSig, b: &SubSig) -> bool {
    a.kind == SubKind::Method
        && b.kind == SubKind::Method
        && a.named == b.named
        && a.ret == b.ret
        && a.params.len() == b.params.len()
        && a.params
            .iter()
            .zip(&b.params)
            .all(|(p, q)| p.named == q.named && p.ty == q.ty && (!p.named || p.name == q.name))
}

/// Element type a literal is checked against when the expected type fixes one: `Optional[...]` is
/// peeled; in a union the literal's container member must be unique; a union containing Any gives
/// no context (Any accepts the literal as inferred).
fn expected_elem(expected: Option<&Type>, array: bool) -> Option<Type> {
    fn peel(mut t: &Type) -> &Type {
        while let Type::Optional(i) = t {
            t = i;
        }
        t
    }
    let elem_of = |t: &Type| match (peel(t), array) {
        (Type::ArrayRef(e), true) | (Type::HashRef(e), false) => Some((**e).clone()),
        _ => None,
    };
    match peel(expected?) {
        Type::Union(ms) if ms.iter().any(|m| matches!(peel(m), Type::Any)) => None,
        Type::Union(ms) => {
            let mut elems = ms.iter().filter_map(elem_of);
            match (elems.next(), elems.next()) {
                (Some(e), None) => Some(e),
                _ => None,
            }
        }
        t => elem_of(t),
    }
}

pub fn check(file: &File, path: &str) -> Result<Facts, Diag> {
    if file.kind == FileKind::Module {
        modules::check_package(file, path)?;
    }
    let mut loader = Loader::default();
    for u in &file.uses {
        loader.get(&u.name)?;
    }
    let own = Rc::new(modules::module_sig(file, path)?);
    let mut c = Checker {
        path,
        file,
        own,
        loader,
        known: modules::known_names(file),
        scopes: Vec::new(),
        current: None,
        facts: Facts::default(),
    };
    for item in &file.items {
        if let Item::Sub(s) = item {
            c.sub(s)?;
        }
    }
    c.current = None;
    c.scopes = vec![HashMap::new()];
    for item in &file.items {
        if let Item::Stmt(s) = item {
            c.stmt(s)?;
        }
    }
    Ok(c.facts)
}

struct Checker<'a> {
    path: &'a str,
    file: &'a File,
    own: Rc<ModuleSig>,
    loader: Loader,
    known: HashSet<String>,
    scopes: Vec<HashMap<String, Type>>,
    current: Option<&'a Sub>,
    facts: Facts,
}

impl<'a> Checker<'a> {
    fn err<T>(&self, span: Span, msg: impl Into<String>) -> R<T> {
        Err(Diag::new(self.path, span, msg))
    }

    fn is_own(&self, package: &str) -> bool {
        self.is_module() && package == self.own.package
    }

    fn current_kind(&self) -> Option<SubKind> {
        let sub = self.current?;
        self.own.subs.get(&sub.name).map(|s| s.kind)
    }

    fn is_module(&self) -> bool {
        self.file.kind == FileKind::Module
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn declare(&mut self, name: &str, ty: Type, span: Span) -> R<()> {
        if self.lookup(name).is_some() {
            return self.err(span, format!("`${name}` is already declared"));
        }
        self.scopes.last_mut().unwrap().insert(name.to_string(), ty);
        Ok(())
    }

    fn check_decl_type(&self, ty: &Type, span: Span) -> R<()> {
        modules::check_type(ty, &self.known, false).map_err(|m| Diag::new(self.path, span, m))
    }

    fn expect_assignable(&mut self, got: &Type, want: &Type, span: Span, ctx: &str) -> R<()> {
        if self.assignable_ctx(got, want)? {
            return Ok(());
        }
        let hint = if *got == Type::Any {
            " (narrow Any with to_int, to_str, to_bool or a legacy class annotation)"
        } else {
            ""
        };
        self.err(
            span,
            format!("{ctx}type mismatch: expected {want}, found {got}{hint}"),
        )
    }

    fn satisfies(&mut self, class: &str, iface: &str) -> R<bool> {
        let Some(iface_sig) = self.sig_of(iface)? else {
            return Ok(false);
        };
        if !iface_sig.is_interface {
            return Ok(false);
        }
        let Some(class_sig) = self.sig_of(class)? else {
            return Ok(false);
        };
        if class_sig.is_interface {
            return Ok(false);
        }
        for (name, want) in &iface_sig.iface {
            match class_sig.subs.get(name) {
                Some(got) if signatures_equal(got, want) => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    fn assignable_ctx(&mut self, from: &Type, to: &Type) -> R<bool> {
        use Type::*;
        match (from, to) {
            (Void, _) | (Class, _) | (_, Void) | (_, Class) => Ok(false),
            (_, Any) => Ok(true),
            (Any, _) => Ok(false),
            (Union(fs), _) => {
                for f in fs {
                    if !self.assignable_ctx(f, to)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (_, Union(ts)) => {
                for t in ts {
                    if self.assignable_ctx(from, t)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (Optional(f), Optional(t)) => self.assignable_ctx(f, t),
            (_, Optional(t)) => self.assignable_ctx(from, t),
            (Optional(_), _) => Ok(false),
            (ArrayRef(f), ArrayRef(t)) | (HashRef(f), HashRef(t)) => self.assignable_ctx(f, t),
            (Object(a), Object(b)) if a == b => Ok(true),
            (Object(a), Object(b)) => Ok(self.satisfies(a, b)?),
            _ => Ok(from == to),
        }
    }

    fn sub(&mut self, s: &'a Sub) -> R<()> {
        self.current = Some(s);
        self.scopes = vec![HashMap::new()];
        // Invocants keep their declared types: `$class: Class` (so misuse fails as `found Class`), `$self: Pkg`.
        for p in &s.params {
            self.declare(&p.name, p.ty.clone(), p.span)?;
        }
        for st in &s.body {
            self.stmt(st)?;
        }
        if s.ret != Type::Void && !terminates(&s.body) {
            return self.err(
                s.span,
                format!("sub `{}` must end with `return` on every path", s.name),
            );
        }
        Ok(())
    }

    fn block(&mut self, stmts: &[Stmt]) -> R<()> {
        self.scopes.push(HashMap::new());
        for s in stmts {
            self.stmt(s)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn stmt(&mut self, st: &Stmt) -> R<()> {
        match st {
            Stmt::My {
                ty,
                name,
                init,
                span,
            } => {
                self.check_decl_type(ty, *span)?;
                let got = self.expr(init, Some(ty))?;
                if got == Type::Any && self.is_legacy_class(ty)? {
                    self.facts.narrow.insert(*span);
                } else {
                    self.expect_assignable(&got, ty, init.span, "")?;
                }
                self.declare(name, ty.clone(), *span)
            }
            Stmt::If { arms, els } => {
                for (c, b) in arms {
                    let t = self.expr(c, None)?;
                    if t != Type::Bool {
                        return self.err(c.span, format!("if condition must be Bool, found {t}"));
                    }
                    self.block(b)?;
                }
                if let Some(b) = els {
                    self.block(b)?;
                }
                Ok(())
            }
            Stmt::Foreach {
                ty,
                var,
                list,
                body,
                span,
            } => {
                self.check_decl_type(ty, *span)?;
                let lt = self.expr(list, Some(&Type::ArrayRef(Box::new(ty.clone()))))?;
                let Type::ArrayRef(elem) = &lt else {
                    return self.err(
                        list.span,
                        format!("foreach requires ArrayRef[T], found {lt}"),
                    );
                };
                self.expect_assignable(elem, ty, list.span, "")?;
                self.scopes.push(HashMap::new());
                self.declare(var, ty.clone(), *span)?;
                for s in body {
                    self.stmt(s)?;
                }
                self.scopes.pop();
                Ok(())
            }
            Stmt::Return { value, span } => {
                let Some(sub) = self.current else {
                    return self.err(*span, "return outside a sub");
                };
                match (value, &sub.ret) {
                    (None, Type::Void) => Ok(()),
                    (None, t) => self.err(*span, format!("must return a value of type {t}")),
                    (Some(v), Type::Void) => self.err(v.span, "Void sub cannot return a value"),
                    (Some(v), t) => {
                        if matches!(v.kind, ExprKind::Bless { .. }) {
                            return self.bless(v);
                        }
                        let got = self.expr(v, Some(t))?;
                        self.expect_assignable(&got, t, v.span, "")
                    }
                }
            }
            Stmt::Die { msg } => {
                let t = self.expr(msg, Some(&Type::Str))?;
                if t != Type::Str {
                    return self.err(msg.span, format!("die requires a Str message, found {t}"));
                }
                Ok(())
            }
            Stmt::Expr(e) => self.expr_any(e, None).map(|_| ()),
        }
    }

    /// Type of a value-position expression (Void rejected).
    fn expr(&mut self, e: &Expr, expected: Option<&Type>) -> R<Type> {
        let t = self.expr_any(e, expected)?;
        if t == Type::Void {
            return self.err(e.span, "Void value cannot be used");
        }
        Ok(t)
    }

    fn expr_any(&mut self, e: &Expr, expected: Option<&Type>) -> R<Type> {
        use Type::*;
        match &e.kind {
            ExprKind::Int(_) => Ok(Int),
            ExprKind::Str(StrLit::Single(_)) => Ok(Str),
            ExprKind::Str(StrLit::Double(parts)) => {
                for p in parts {
                    if let StrPart::Var(n, sp) = p {
                        match self.lookup(n) {
                            None => return self.err(*sp, format!("undeclared variable `${n}`")),
                            Some(Str) => {}
                            Some(t) => {
                                return self.err(
                                    *sp,
                                    format!(
                                        "only Str variables can be interpolated; `${n}` is {t}"
                                    ),
                                )
                            }
                        }
                    }
                }
                Ok(Str)
            }
            ExprKind::Var(n) => match self.lookup(n) {
                Some(t) => Ok(t.clone()),
                None => self.err(e.span, format!("undeclared variable `${n}`")),
            },
            ExprKind::Array(items) => {
                let values: Vec<&Expr> = items.iter().collect();
                self.literal(&values, expected, e.span, true)
            }
            ExprKind::Hash(pairs) => {
                let mut seen = HashSet::new();
                for p in pairs {
                    if !seen.insert(p.key.as_str()) {
                        return self.err(p.span, format!("duplicate key `{}`", p.key));
                    }
                }
                let values: Vec<&Expr> = pairs.iter().map(|p| &p.value).collect();
                self.literal(&values, expected, e.span, false)
            }
            ExprKind::Neg(x) => {
                let t = self.expr(x, Some(&Int))?;
                if t != Int {
                    return self.err(
                        e.span,
                        format!("unary `-` requires an Int operand, found {t}"),
                    );
                }
                Ok(Int)
            }
            ExprKind::Binary(op, l, r) => {
                let (want, out) = match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul => (Int, Int),
                    BinOp::Concat => (Str, Str),
                    BinOp::NumEq => (Int, Bool),
                    BinOp::StrEq => (Str, Bool),
                };
                let lt = self.expr(l, Some(&want))?;
                let rt = self.expr(r, Some(&want))?;
                if lt != want || rt != want {
                    return self.err(
                        e.span,
                        format!(
                            "operator `{}` requires {want} operands, found {lt} and {rt}",
                            op.symbol()
                        ),
                    );
                }
                Ok(out)
            }
            ExprKind::Call { name, args } => self.call(name, args, e.span),
            ExprKind::ClassCall {
                class,
                method,
                args,
            } => self.class_call(class, method, args, e.span),
            ExprKind::MethodCall { recv, method, args } => {
                self.method_call(recv, method, args, e.span)
            }
            ExprKind::Field { recv, name } => self.field(recv, name, e.span),
            ExprKind::Bless { .. } => self.err(e.span, "bless must appear directly in `return`"),
        }
    }

    fn literal(
        &mut self,
        values: &[&Expr],
        expected: Option<&Type>,
        span: Span,
        array: bool,
    ) -> R<Type> {
        let wrap = |t: Type| {
            if array {
                Type::ArrayRef(Box::new(t))
            } else {
                Type::HashRef(Box::new(t))
            }
        };
        if let Some(t) = expected_elem(expected, array) {
            for v in values {
                let got = self.expr(v, Some(&t))?;
                self.expect_assignable(&got, &t, v.span, "")?;
            }
            return Ok(wrap(t));
        }
        let Some(first) = values.first() else {
            return self.err(span, "cannot infer the type of an empty literal");
        };
        let t = self.expr(first, None)?;
        for v in &values[1..] {
            if self.expr(v, None)? != t {
                let what = if array {
                    "array literal elements"
                } else {
                    "hash literal values"
                };
                return self.err(v.span, format!("{what} must all have the same type"));
            }
        }
        Ok(wrap(t))
    }

    /// Type of a call into legacy Perl: arguments must be non-Void values with no expected type, and the result is Any.
    fn legacy_call(&mut self, args: &Args) -> R<Type> {
        match args {
            Args::Positional(v) => v.iter().try_for_each(|e| self.expr(e, None).map(|_| ()))?,
            Args::Named(p) => p
                .iter()
                .try_for_each(|p| self.expr(&p.value, None).map(|_| ()))?,
        }
        Ok(Type::Any)
    }

    fn call(&mut self, name: &str, args: &Args, span: Span) -> R<Type> {
        if let Some(t) = conversion_type(name) {
            return match args {
                Args::Positional(a) if a.len() == 1 => {
                    self.expr(&a[0], None)?;
                    Ok(t)
                }
                _ => self.err(span, format!("{name} takes exactly one argument")),
            };
        }
        let (pkg, fname) = match name.rsplit_once("::") {
            Some((p, f)) => (Some(p), f),
            None => (None, name),
        };
        let m = match pkg {
            None => self.own.clone(),
            Some(p) => match self.class_sig(p, span)? {
                Some(m) => m,
                None => return self.legacy_call(args),
            },
        };
        let Some(s) = m.subs.get(fname) else {
            return self.err(span, format!("unknown function `{name}`"));
        };
        let qual = format!("{}::{fname}", m.package);
        match s.kind {
            SubKind::Constructor => self.err(
                span,
                format!(
                    "{qual} is a constructor; call it as `{}->{fname}(...)`",
                    m.package
                ),
            ),
            SubKind::Method => self.err(
                span,
                format!("{qual} is a method; call it as `$obj->{fname}(...)`"),
            ),
            SubKind::Function => {
                self.args(s, name, args, span)?;
                Ok(s.ret.clone())
            }
        }
    }

    fn args(&mut self, sig: &SubSig, display: &str, args: &Args, span: Span) -> R<()> {
        match args {
            Args::Positional(a) if sig.named && a.is_empty() => {
                self.named_args(sig, display, &[], span)
            }
            Args::Positional(_) if sig.named => self.err(
                span,
                format!("{display} takes named arguments (name => value)"),
            ),
            Args::Named(_) if !sig.named => {
                self.err(span, format!("{display} takes positional arguments"))
            }
            Args::Positional(a) => {
                if a.len() != sig.params.len() {
                    return self.err(
                        span,
                        format!(
                            "{display} expects {} arguments, found {}",
                            sig.params.len(),
                            a.len()
                        ),
                    );
                }
                for (i, (v, p)) in a.iter().zip(&sig.params).enumerate() {
                    let got = self.expr(v, Some(&p.ty))?;
                    self.expect_assignable(
                        &got,
                        &p.ty,
                        v.span,
                        &format!("argument {} of {display}: ", i + 1),
                    )?;
                }
                Ok(())
            }
            Args::Named(pairs) => self.named_args(sig, display, pairs, span),
        }
    }

    fn named_args(&mut self, sig: &SubSig, display: &str, pairs: &[Pair], span: Span) -> R<()> {
        let mut seen = HashSet::new();
        for pr in pairs {
            if !seen.insert(pr.key.as_str()) {
                return self.err(pr.span, format!("duplicate named argument `{}`", pr.key));
            }
            let Some(p) = sig.params.iter().find(|p| p.name == pr.key) else {
                return self.err(
                    pr.span,
                    format!("unknown named argument `{}` for {display}", pr.key),
                );
            };
            let got = self.expr(&pr.value, Some(&p.ty))?;
            self.expect_assignable(
                &got,
                &p.ty,
                pr.value.span,
                &format!("named argument `{}` of {display}: ", pr.key),
            )?;
        }
        for p in &sig.params {
            if !seen.contains(p.name.as_str()) && !matches!(p.ty, Type::Optional(_)) {
                return self.err(
                    span,
                    format!("missing required named argument `{}` for {display}", p.name),
                );
            }
        }
        Ok(())
    }

    /// Declared type is a class that is `use`d, not this module, and has no .tpm.
    fn is_legacy_class(&mut self, ty: &Type) -> R<bool> {
        let Type::Object(c) = ty else {
            return Ok(false);
        };
        Ok(self.known.contains(c) && self.sig_of(c)?.is_none())
    }

    /// Own package's signature, else the loader's. `None` = legacy Perl.
    fn sig_of(&mut self, class: &str) -> R<Option<Rc<ModuleSig>>> {
        if self.is_own(class) {
            return Ok(Some(self.own.clone()));
        }
        self.loader.get(class)
    }

    /// Signature of a class written by name in this file. `None` = legacy Perl.
    fn class_sig(&mut self, class: &str, span: Span) -> R<Option<Rc<ModuleSig>>> {
        if !self.known.contains(class) {
            return self.err(
                span,
                format!("package `{class}` is not used (add `use {class};`)"),
            );
        }
        self.sig_of(class)
    }

    fn class_call(&mut self, class: &str, method: &str, args: &Args, span: Span) -> R<Type> {
        let Some(m) = self.class_sig(class, span)? else {
            return self.legacy_call(args);
        };
        let Some(s) = m.subs.get(method) else {
            return self.err(span, format!("{class} has no method `{method}`"));
        };
        if s.kind != SubKind::Constructor {
            return self.err(span, format!("{class}::{method} is not a constructor (its first parameter is not `Class $class`)"));
        }
        self.args(s, &format!("{class}::{method}"), args, span)?;
        Ok(s.ret.clone())
    }

    fn method_call(&mut self, recv: &Expr, method: &str, args: &Args, span: Span) -> R<Type> {
        let rt = self.expr(recv, None)?;
        let Type::Object(c) = &rt else {
            if rt == Type::Any {
                return self.err(span, "cannot call a method on Any (narrow it first)");
            }
            return self.err(span, format!("cannot call a method on {rt}"));
        };
        // Receiver types come from checked declarations, so no `use` is required here.
        let m = self.sig_of(c)?;
        let Some(m) = m else {
            return self.legacy_call(args);
        };
        if m.is_interface {
            let Some(s) = m.iface.get(method) else {
                return self.err(span, format!("{c} has no method `{method}`"));
            };
            self.args(s, &format!("{c}::{method}"), args, span)?;
            return Ok(s.ret.clone());
        }
        let Some(s) = m.subs.get(method) else {
            return self.err(span, format!("{c} has no method `{method}`"));
        };
        match s.kind {
            SubKind::Constructor => self.err(
                span,
                format!("constructor `{method}` must be called on the class: `{c}->{method}(...)`"),
            ),
            SubKind::Function => self.err(
                span,
                format!("`{method}` is a function, not a method; call it as `{c}::{method}(...)`"),
            ),
            SubKind::Method => {
                self.args(s, &format!("{c}::{method}"), args, span)?;
                Ok(s.ret.clone())
            }
        }
    }

    fn field(&mut self, recv: &Expr, name: &str, span: Span) -> R<Type> {
        let rt = self.expr(recv, None)?;
        match &rt {
            Type::HashRef(_) => self.err(span, "element access on HashRef is not supported"),
            Type::Object(c) if self.is_own(c) => {
                let in_method = matches!(
                    self.current_kind(),
                    Some(SubKind::Method | SubKind::Constructor)
                );
                if !in_method {
                    return self.err(
                        span,
                        format!("fields of {c} can only be read in a method of {c}"),
                    );
                }
                match self.own.fields.iter().find(|(n, _)| n == name) {
                    Some((_, t)) => Ok(t.clone()),
                    None => self.err(span, format!("unknown field `{name}` on {c}")),
                }
            }
            Type::Object(c) => self.err(span, format!("fields of {c} are private to package {c}")),
            t => self.err(span, format!("cannot access a field of {t}")),
        }
    }

    /// `return bless({...}, $class);` inside a constructor.
    fn bless(&mut self, e: &Expr) -> R<()> {
        let ExprKind::Bless { fields, target } = &e.kind else {
            unreachable!("caller checks for Bless")
        };
        let is_ctor = self.current_kind() == Some(SubKind::Constructor);
        if !is_ctor {
            return self.err(
                e.span,
                "bless is only allowed in a constructor (first parameter `Class $class`)",
            );
        }
        if !matches!(&target.kind, ExprKind::Var(v) if v == "class") {
            return self.err(
                target.span,
                "bless target must be the constructor's `$class`",
            );
        }
        let own = self.own.clone();
        let mut seen = HashSet::new();
        for pr in fields {
            if !seen.insert(pr.key.as_str()) {
                return self.err(pr.span, format!("duplicate key `{}`", pr.key));
            }
            let Some((_, fty)) = own.fields.iter().find(|(n, _)| *n == pr.key) else {
                return self.err(
                    pr.span,
                    format!("unknown field `{}` in bless for {}", pr.key, own.package),
                );
            };
            let got = self.expr(&pr.value, Some(fty))?;
            self.expect_assignable(
                &got,
                fty,
                pr.value.span,
                &format!("field `{}` of {}: ", pr.key, own.package),
            )?;
        }
        for (n, t) in &own.fields {
            if !seen.contains(n.as_str()) && !matches!(t, Type::Optional(_)) {
                return self.err(
                    e.span,
                    format!("missing field `{n}` in bless for {}", own.package),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Type::*;

    fn b(t: Type) -> Box<Type> {
        Box::new(t)
    }

    #[test]
    fn assignability() {
        let file = File {
            kind: FileKind::Script,
            package: None,
            uses: vec![],
            items: vec![],
            interface: None,
        };
        let mut c = Checker {
            path: "test",
            file: &file,
            own: Rc::new(ModuleSig {
                package: String::new(),
                subs: HashMap::new(),
                fields: Vec::new(),
                is_interface: false,
                iface: HashMap::new(),
            }),
            loader: Loader::default(),
            known: HashSet::new(),
            scopes: Vec::new(),
            current: None,
            facts: Facts::default(),
        };
        // Different-named objects need the loader; covered by tests/typecheck.rs rejects_*.
        let mut ok = |from: &Type, to: &Type| c.assignable_ctx(from, to).unwrap();
        assert!(ok(&Int, &Int));
        assert!(!ok(&Int, &Str));
        assert!(ok(&Int, &Any));
        assert!(!ok(&Any, &Str));
        assert!(ok(&Any, &Any));
        assert!(!ok(&Class, &Any));
        assert!(!ok(&Void, &Any));
        assert!(ok(&Int, &Union(vec![Int, Str])));
        assert!(!ok(&Union(vec![Int, Str]), &Int));
        assert!(ok(&Int, &Optional(b(Int))));
        assert!(ok(&Optional(b(Int)), &Optional(b(Int))));
        assert!(!ok(&Optional(b(Int)), &Int));
        assert!(ok(
            &Optional(b(Int)),
            &Union(vec![Optional(b(Int)), Str])
        ));
        assert!(ok(
            &ArrayRef(b(Int)),
            &ArrayRef(b(Union(vec![Int, Str])))
        ));
        assert!(!ok(&ArrayRef(b(Int)), &HashRef(b(Int))));
        assert!(ok(&Object("Point".into()), &Object("Point".into())));
    }
}
