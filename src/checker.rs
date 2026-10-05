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

pub fn assignable(from: &Type, to: &Type) -> bool {
    use Type::*;
    match (from, to) {
        (Void, _) | (Class, _) | (_, Void) | (_, Class) => false,
        (_, Any) => true,
        (Any, _) => false,
        (Union(fs), _) => fs.iter().all(|f| assignable(f, to)),
        (_, Union(ts)) => ts.iter().any(|t| assignable(from, t)),
        (Optional(f), Optional(t)) => assignable(f, t),
        (_, Optional(t)) => assignable(from, t),
        (Optional(_), _) => false,
        (ArrayRef(f), ArrayRef(t)) | (HashRef(f), HashRef(t)) => assignable(f, t),
        _ => from == to,
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

    fn expect_assignable(&self, got: &Type, want: &Type, span: Span, ctx: &str) -> R<()> {
        if assignable(got, want) {
            return Ok(());
        }
        let hint = if *got == Type::Any { " (narrow Any with to_int, to_str, to_bool or a legacy class annotation)" } else { "" };
        self.err(span, format!("{ctx}type mismatch: expected {want}, found {got}{hint}"))
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
            return self.err(s.span, format!("sub `{}` must end with `return` on every path", s.name));
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
            Stmt::My { ty, name, init, span } => {
                self.check_decl_type(ty, *span)?;
                let got = self.expr(init, Some(ty))?;
                if got == Type::Any && self.is_legacy_class(ty)? {
                    self.facts.narrow.insert(*span);
                } else {
                    self.expect_assignable(&got, ty, init.span, "")?;
                }
                self.declare(name, ty.clone(), *span)
            }
            Stmt::If { arms, els, .. } => {
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
            Stmt::Foreach { ty, var, list, body, span } => {
                self.check_decl_type(ty, *span)?;
                let lt = self.expr(list, Some(&Type::ArrayRef(Box::new(ty.clone()))))?;
                let Type::ArrayRef(elem) = &lt else {
                    return self.err(list.span, format!("foreach requires ArrayRef[T], found {lt}"));
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
                let Some(sub) = self.current else { return self.err(*span, "return outside a sub") };
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
            Stmt::Die { msg, .. } => {
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
                            Some(t) => return self.err(*sp, format!("only Str variables can be interpolated; `${n}` is {t}")),
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
                    return self.err(e.span, format!("unary `-` requires an Int operand, found {t}"));
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
                    return self.err(e.span, format!("operator `{}` requires {want} operands, found {lt} and {rt}", op.symbol()));
                }
                Ok(out)
            }
            ExprKind::Call { name, args } => self.call(name, args, e.span),
            ExprKind::ClassCall { class, method, args } => self.class_call(class, method, args, e.span),
            ExprKind::MethodCall { recv, method, args } => self.method_call(recv, method, args, e.span),
            ExprKind::Field { recv, name } => self.field(recv, name, e.span),
            ExprKind::Bless { .. } => self.err(e.span, "bless must appear directly in `return`"),
        }
    }

    fn literal(&mut self, values: &[&Expr], expected: Option<&Type>, span: Span, array: bool) -> R<Type> {
        let wrap = |t: Type| if array { Type::ArrayRef(Box::new(t)) } else { Type::HashRef(Box::new(t)) };
        let elem = match (expected, array) {
            (Some(Type::ArrayRef(t)), true) | (Some(Type::HashRef(t)), false) => Some((**t).clone()),
            _ => None,
        };
        if let Some(t) = elem {
            for v in values {
                let got = self.expr(v, Some(&t))?;
                self.expect_assignable(&got, &t, v.span, "")?;
            }
            return Ok(wrap(t));
        }
        let Some(first) = values.first() else { return self.err(span, "cannot infer the type of an empty literal") };
        let t = self.expr(first, None)?;
        for v in &values[1..] {
            if self.expr(v, None)? != t {
                let what = if array { "array literal elements" } else { "hash literal values" };
                return self.err(v.span, format!("{what} must all have the same type"));
            }
        }
        Ok(wrap(t))
    }

    /// Args of a legacy call: any non-Void value, no expected types.
    fn legacy_args(&mut self, args: &Args) -> R<()> {
        match args {
            Args::Positional(v) => v.iter().try_for_each(|e| self.expr(e, None).map(|_| ())),
            Args::Named(p) => p.iter().try_for_each(|p| self.expr(&p.value, None).map(|_| ())),
        }
    }

    fn call(&mut self, name: &str, args: &Args, span: Span) -> R<Type> {
        let conv = match name {
            "to_int" => Some(Type::Int),
            "to_str" => Some(Type::Str),
            "to_bool" => Some(Type::Bool),
            _ => None,
        };
        if let Some(t) = conv {
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
            Some(p) if self.is_module() && p == self.own.package => self.own.clone(),
            Some(p) => {
                if !self.known.contains(p) {
                    return self.err(span, format!("package `{p}` is not used (add `use {p};`)"));
                }
                match self.loader.get(p)? {
                    Some(m) => m,
                    None => {
                        self.legacy_args(args)?;
                        return Ok(Type::Any);
                    }
                }
            }
        };
        let Some(s) = m.subs.get(fname) else { return self.err(span, format!("unknown function `{name}`")) };
        let qual = format!("{}::{fname}", m.package);
        match s.kind {
            SubKind::Constructor => self.err(span, format!("{qual} is a constructor; call it as `{}->{fname}(...)`", m.package)),
            SubKind::Method => self.err(span, format!("{qual} is a method; call it as `$obj->{fname}(...)`")),
            SubKind::Function => {
                self.args(s, name, args, span)?;
                Ok(s.ret.clone())
            }
        }
    }

    fn args(&mut self, sig: &SubSig, display: &str, args: &Args, span: Span) -> R<()> {
        match args {
            Args::Positional(a) if sig.named && a.is_empty() => self.named_args(sig, display, &[], span),
            Args::Positional(_) if sig.named => self.err(span, format!("{display} takes named arguments (name => value)")),
            Args::Named(_) if !sig.named => self.err(span, format!("{display} takes positional arguments")),
            Args::Positional(a) => {
                if a.len() != sig.params.len() {
                    return self.err(span, format!("{display} expects {} arguments, found {}", sig.params.len(), a.len()));
                }
                for (i, (v, p)) in a.iter().zip(&sig.params).enumerate() {
                    let got = self.expr(v, Some(&p.ty))?;
                    self.expect_assignable(&got, &p.ty, v.span, &format!("argument {} of {display}: ", i + 1))?;
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
                return self.err(pr.span, format!("unknown named argument `{}` for {display}", pr.key));
            };
            let got = self.expr(&pr.value, Some(&p.ty))?;
            self.expect_assignable(&got, &p.ty, pr.value.span, &format!("named argument `{}` of {display}: ", pr.key))?;
        }
        for p in &sig.params {
            if !seen.contains(p.name.as_str()) && !matches!(p.ty, Type::Optional(_)) {
                return self.err(span, format!("missing required named argument `{}` for {display}", p.name));
            }
        }
        Ok(())
    }

    // Part B replaces these four with the real implementations.
    fn is_legacy_class(&mut self, _ty: &Type) -> R<bool> {
        Ok(false)
    }
    fn class_call(&mut self, _c: &str, _m: &str, _a: &Args, span: Span) -> R<Type> {
        self.err(span, "objects are not implemented yet")
    }
    fn method_call(&mut self, _r: &Expr, _m: &str, _a: &Args, span: Span) -> R<Type> {
        self.err(span, "objects are not implemented yet")
    }
    fn field(&mut self, _r: &Expr, _n: &str, span: Span) -> R<Type> {
        self.err(span, "objects are not implemented yet")
    }
    fn bless(&mut self, e: &Expr) -> R<()> {
        self.err(e.span, "objects are not implemented yet")
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
        assert!(assignable(&Int, &Int));
        assert!(!assignable(&Int, &Str));
        assert!(assignable(&Int, &Any));
        assert!(!assignable(&Any, &Str));
        assert!(assignable(&Any, &Any));
        assert!(!assignable(&Class, &Any));
        assert!(!assignable(&Void, &Any));
        assert!(assignable(&Int, &Union(vec![Int, Str])));
        assert!(!assignable(&Union(vec![Int, Str]), &Int));
        assert!(assignable(&Int, &Optional(b(Int))));
        assert!(assignable(&Optional(b(Int)), &Optional(b(Int))));
        assert!(!assignable(&Optional(b(Int)), &Int));
        assert!(assignable(&Optional(b(Int)), &Union(vec![Optional(b(Int)), Str])));
        assert!(assignable(&ArrayRef(b(Int)), &ArrayRef(b(Union(vec![Int, Str])))));
        assert!(!assignable(&ArrayRef(b(Int)), &HashRef(b(Int))));
        assert!(assignable(&Object("Point".into()), &Object("Point".into())));
        assert!(!assignable(&Object("Point::Label".into()), &Object("Point".into())));
    }
}
