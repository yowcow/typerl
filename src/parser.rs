use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex, Tok, Token};

type R<T> = Result<T, Diag>;

const WORD_OPS: &[&str] = &["ne", "lt", "gt", "le", "ge", "cmp", "and", "or", "not", "xor", "x", "isa"];
/// Max nesting of expressions, blocks and types; keeps the AST height (and every recursive walk over it) bounded.
const MAX_DEPTH: usize = 64;
/// Max operators in one left-associative chain (`a + b + ...`, `a->f->g...`).
const MAX_CHAIN: usize = 256;
const MODIFIERS: &[&str] = &["if", "unless", "while", "until", "for", "foreach"];

/// Words that are rejected wherever a statement or a term starts.
fn banned(word: &str) -> Option<String> {
    Some(match word {
        "eval" | "evalbytes" => "eval is not supported (no string eval or runtime code)".into(),
        "require" => "require is not supported; use a literal `use Name;` at the top of the file".into(),
        "BEGIN" | "CHECK" | "INIT" | "END" | "UNITCHECK" => "BEGIN/CHECK/INIT/END blocks are not supported".into(),
        "tie" | "tied" | "untie" => "tie is not supported".into(),
        "local" | "our" | "state" => format!("`{word}` declarations are not supported; use `my T $x`"),
        "wantarray" => "wantarray is not supported (no list context)".into(),
        "undef" => "undef is not supported".into(),
        "q" | "qq" | "qw" | "qr" | "qx" | "m" | "s" | "tr" | "y" => "quote-like operators are not supported".into(),
        "for" => "`for` is not supported; use `foreach my T $v (...)`".into(),
        "unless" | "while" | "until" | "do" | "last" | "next" | "redo" | "goto" | "no" | "format" => {
            format!("`{word}` is not supported")
        }
        _ => return None,
    })
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) | Tok::Int(s) => format!("`{s}`"),
        Tok::Var(s) => format!("`${s}`"),
        Tok::Str(_) => "a string".into(),
        Tok::Punct(p) => format!("`{p}`"),
        Tok::Eof => "end of file".into(),
    }
}

pub fn parse(file: &str, src: &str, kind: FileKind) -> Result<File, Diag> {
    let toks = lex(file, src)?;
    Parser { file, toks, pos: 0, kind, depth: 0 }.file()
}

struct Parser<'a> {
    file: &'a str,
    toks: Vec<Token>,
    pos: usize,
    kind: FileKind,
    depth: usize,
}

impl Parser<'_> {
    fn tok(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn tok_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos].span
    }

    fn advance(&mut self) -> Span {
        let span = self.toks[self.pos].span;
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        span
    }

    fn err<T>(&self, span: Span, msg: impl Into<String>) -> R<T> {
        Err(Diag::new(self.file, span, msg))
    }

    /// Runs `f` one nesting level deeper, failing past `MAX_DEPTH`.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.depth >= MAX_DEPTH {
            return self.err(self.span(), format!("nesting is too deep (limit {MAX_DEPTH})"));
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    /// Counts one more operator in a chain, failing past `MAX_CHAIN`.
    fn chain(&self, n: &mut usize) -> R<()> {
        *n += 1;
        if *n > MAX_CHAIN {
            return self.err(self.span(), format!("expression is too long (limit {MAX_CHAIN})"));
        }
        Ok(())
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.tok(), Tok::Punct(q) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.tok(), Tok::Ident(s) if s == w)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        let hit = self.is_punct(p);
        if hit {
            self.advance();
        }
        hit
    }

    fn unexpected<T>(&self, what: &str) -> R<T> {
        let span = self.span();
        if self.is_punct("|") {
            // `|` is a type-union separator only.
            return self.err(span, "operator `|` is not supported");
        }
        if let Tok::Ident(w) = self.tok() {
            if MODIFIERS.contains(&w.as_str()) {
                return self.err(span, "statement modifiers are not supported");
            }
            if WORD_OPS.contains(&w.as_str()) {
                return self.err(span, format!("operator `{w}` is not supported"));
            }
        }
        self.err(span, format!("expected {what}, found {}", describe(self.tok())))
    }

    fn expect_punct(&mut self, p: &str) -> R<Span> {
        if self.is_punct(p) {
            return Ok(self.advance());
        }
        self.unexpected(&format!("`{p}`"))
    }

    fn eat_ident(&mut self) -> Option<String> {
        let Tok::Ident(s) = self.tok() else { return None };
        let s = s.clone();
        self.advance();
        Some(s)
    }

    fn name(&mut self, what: &str) -> R<String> {
        match self.eat_ident() {
            Some(s) => Ok(s),
            None => self.unexpected(what),
        }
    }

    fn var_name(&mut self, what: &str) -> R<String> {
        match self.tok().clone() {
            Tok::Var(v) => {
                self.advance();
                Ok(v)
            }
            _ => self.unexpected(what),
        }
    }

    fn bare_key(&mut self, msg: &str) -> R<String> {
        match self.tok().clone() {
            Tok::Ident(k) if !k.contains("::") => {
                self.advance();
                Ok(k)
            }
            _ => self.err(self.span(), msg),
        }
    }

    fn missing_type_if_var(&self) -> R<()> {
        if let Tok::Var(v) = self.tok() {
            return self.err(self.span(), format!("missing type annotation for `${v}`"));
        }
        Ok(())
    }

    fn misplaced(&self, w: &str) -> Option<String> {
        match w {
            "use" => Some("use must appear at the top of the file".into()),
            "package" => Some(match self.kind {
                FileKind::Module => "only one package per module is allowed",
                FileKind::Script => "package is not allowed in a script (.tpr)",
            }.into()),
            "field" => Some("field is only allowed at the top level of a module (.tpm)".into()),
            _ => banned(w),
        }
    }

    // ---- file structure ----

    fn file(&mut self) -> R<File> {
        let mut f = File { kind: self.kind, package: None, uses: vec![], items: vec![] };
        if self.kind == FileKind::Module {
            if !self.is_word("package") {
                return self.err(self.span(), "a module must start with `package Name;`");
            }
            let span = self.advance();
            let name = self.name("a package name")?;
            self.expect_punct(";")?;
            f.package = Some((name, span));
        }
        while self.is_word("use") {
            f.uses.push(self.use_decl()?);
        }
        while *self.tok() != Tok::Eof {
            let span = self.span();
            if let Tok::Ident(w) = self.tok().clone() {
                match w.as_str() {
                    "field" if self.kind == FileKind::Module => {
                        f.items.push(Item::Field(self.field_decl()?));
                        continue;
                    }
                    "sub" => {
                        f.items.push(Item::Sub(self.sub_decl()?));
                        continue;
                    }
                    _ => {
                        if let Some(m) = self.misplaced(&w) {
                            return self.err(span, m);
                        }
                    }
                }
            }
            if self.kind == FileKind::Module {
                return self.err(span, "module top level may only contain field and sub declarations");
            }
            f.items.push(Item::Stmt(self.stmt()?));
        }
        Ok(f)
    }

    fn use_decl(&mut self) -> R<Use> {
        let span = self.advance();
        let Some(name) = self.eat_ident() else {
            return self.err(self.span(), "use requires a literal module name");
        };
        match name.as_str() {
            "strict" | "warnings" => {
                return self.err(span, "`use strict` and `use warnings` are emitted automatically; remove them")
            }
            "parent" | "base" => return self.err(span, "inheritance is not supported (`use parent` / `use base`)"),
            _ => {}
        }
        if name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return self.err(span, format!("pragmas are not supported (`use {name}`)"));
        }
        if !self.eat_punct(";") {
            return self.err(self.span(), "import lists are not supported");
        }
        Ok(Use { name })
    }

    fn field_decl(&mut self) -> R<Field> {
        let span = self.advance();
        let Some(name) = self.eat_ident() else {
            return self.err(self.span(), "field syntax is `field name: Type;`");
        };
        if self.is_punct(";") {
            return self.err(span, format!("missing type annotation for field `{name}`"));
        }
        self.expect_punct(":")?;
        let ty = self.ty()?;
        if self.is_punct(":") {
            let sp = self.advance();
            if self.is_word("reader") || self.is_word("writer") {
                return self.err(sp, ":reader and :writer are not supported (no automatic accessors)");
            }
            return self.err(sp, "field attributes are not supported");
        }
        self.expect_punct(";")?;
        Ok(Field { name, ty, span })
    }

    fn sub_decl(&mut self) -> R<Sub> {
        let span = self.advance();
        let Some(name) = self.eat_ident() else {
            return self.err(span, "anonymous subs and closures are not supported");
        };
        if name.contains("::") {
            return self.err(span, "sub names must not be qualified");
        }
        if self.is_punct(":") {
            return self.err(self.span(), "sub attributes are not supported");
        }
        if !self.eat_punct("(") {
            return self.err(self.span(), "missing parameter list `(...)`");
        }
        let mut params = Vec::new();
        if !self.is_punct(")") {
            loop {
                params.push(self.param()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
        }
        self.expect_punct(")")?;
        if !self.eat_punct("->") {
            return self.err(self.span(), "missing return type annotation (`-> Type`)");
        }
        let ret = self.ty()?;
        let body = self.block()?;
        Ok(Sub { name, params, ret, body, span })
    }

    fn param(&mut self) -> R<Param> {
        let span = self.span();
        let named = self.eat_punct(":");
        self.missing_type_if_var()?;
        let ty = self.ty()?;
        let name = self.var_name("a parameter variable")?;
        Ok(Param { name, ty, named, span })
    }

    fn ty(&mut self) -> R<Type> {
        self.nested(Self::ty_union)
    }

    fn ty_union(&mut self) -> R<Type> {
        let mut alts = vec![self.ty_atom()?];
        while self.eat_punct("|") {
            alts.push(self.ty_atom()?);
        }
        Ok(if alts.len() == 1 { alts.pop().unwrap() } else { Type::Union(alts) })
    }

    fn ty_atom(&mut self) -> R<Type> {
        let span = self.span();
        let name = self.name("a type")?;
        let simple = match name.as_str() {
            "Int" => Some(Type::Int),
            "Str" => Some(Type::Str),
            "Bool" => Some(Type::Bool),
            "Any" => Some(Type::Any),
            "Void" => Some(Type::Void),
            "Class" => Some(Type::Class),
            _ => None,
        };
        if let Some(t) = simple {
            return Ok(t);
        }
        if matches!(name.as_str(), "ArrayRef" | "HashRef" | "Optional") {
            if !self.eat_punct("[") {
                return self.err(span, format!("`{name}` requires a type parameter: `{name}[T]`"));
            }
            let inner = Box::new(self.ty()?);
            self.expect_punct("]")?;
            return Ok(match name.as_str() {
                "ArrayRef" => Type::ArrayRef(inner),
                "HashRef" => Type::HashRef(inner),
                _ => Type::Optional(inner),
            });
        }
        if self.is_punct("[") {
            return self.err(span, format!("`{name}` does not take type parameters"));
        }
        Ok(Type::Object(name))
    }

    // ---- statements ----

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.nested(Self::block_body)
    }

    fn block_body(&mut self) -> R<Vec<Stmt>> {
        self.expect_punct("{")?;
        let mut out = Vec::new();
        while !self.is_punct("}") {
            if *self.tok() == Tok::Eof {
                return self.unexpected("`}`");
            }
            out.push(self.stmt()?);
        }
        self.advance();
        Ok(out)
    }

    fn stmt(&mut self) -> R<Stmt> {
        let span = self.span();
        if self.is_punct("{") {
            return self.err(span, "bare blocks are not supported");
        }
        if let Tok::Ident(w) = self.tok().clone() {
            match w.as_str() {
                "my" => return self.my_stmt(),
                "if" => return self.if_stmt(),
                "foreach" => return self.foreach_stmt(),
                "return" => {
                    self.advance();
                    let value = if self.is_punct(";") { None } else { Some(self.expr()?) };
                    self.expect_punct(";")?;
                    return Ok(Stmt::Return { value, span });
                }
                "die" => {
                    self.advance();
                    let msg = self.expr()?;
                    self.expect_punct(";")?;
                    return Ok(Stmt::Die { msg });
                }
                "sub" => {
                    let msg = if matches!(self.tok_at(1), Tok::Ident(_)) {
                        "nested subs are not supported"
                    } else {
                        "anonymous subs and closures are not supported"
                    };
                    return self.err(span, msg);
                }
                _ => {
                    if let Some(m) = self.misplaced(&w) {
                        return self.err(span, m);
                    }
                }
            }
        }
        let e = self.expr()?;
        if self.is_punct("=") {
            let msg = if matches!(e.kind, ExprKind::Field { .. }) {
                "field assignment is only allowed in the constructor's bless hash literal"
            } else {
                "reassignment is not supported"
            };
            return self.err(span, msg);
        }
        if !matches!(e.kind, ExprKind::Call { .. } | ExprKind::ClassCall { .. } | ExprKind::MethodCall { .. }) {
            if !self.is_punct(";") {
                return self.unexpected("`;`");
            }
            return self.err(span, "only calls can be used as statements");
        }
        self.expect_punct(";")?;
        Ok(Stmt::Expr(e))
    }

    fn my_stmt(&mut self) -> R<Stmt> {
        let span = self.advance();
        if self.is_punct("(") {
            return self.err(span, "list assignment is not supported");
        }
        self.missing_type_if_var()?;
        let ty = self.ty()?;
        let name = self.var_name("a variable")?;
        if self.is_punct(";") {
            return self.err(span, "variables must be initialized: `my T $x = ...;`");
        }
        self.expect_punct("=")?;
        let init = self.expr()?;
        self.expect_punct(";")?;
        Ok(Stmt::My { ty, name, init, span })
    }

    fn cond(&mut self) -> R<Expr> {
        self.expect_punct("(")?;
        let e = self.expr()?;
        self.expect_punct(")")?;
        Ok(e)
    }

    fn if_stmt(&mut self) -> R<Stmt> {
        self.advance();
        let c = self.cond()?;
        let b = self.block()?;
        let mut arms = vec![(c, b)];
        while self.is_word("elsif") {
            self.advance();
            let c = self.cond()?;
            let b = self.block()?;
            arms.push((c, b));
        }
        let els = if self.is_word("else") {
            self.advance();
            Some(self.block()?)
        } else {
            None
        };
        Ok(Stmt::If { arms, els })
    }

    fn foreach_stmt(&mut self) -> R<Stmt> {
        let span = self.advance();
        if !self.is_word("my") {
            return self.err(self.span(), "foreach requires `my T $var`");
        }
        self.advance();
        self.missing_type_if_var()?;
        let ty = self.ty()?;
        let var = self.var_name("a loop variable")?;
        let list = self.cond()?;
        let body = self.block()?;
        Ok(Stmt::Foreach { ty, var, list, body, span })
    }

    // ---- expressions ----

    fn expr(&mut self) -> R<Expr> {
        self.nested(Self::comparison)
    }

    fn comparison(&mut self) -> R<Expr> {
        let left = self.additive()?;
        let op = if self.is_punct("==") {
            BinOp::NumEq
        } else if self.is_word("eq") {
            BinOp::StrEq
        } else {
            return Ok(left);
        };
        self.advance();
        let right = self.additive()?;
        if self.is_punct("==") || self.is_word("eq") {
            return self.err(self.span(), "chained comparisons are not supported");
        }
        Ok(bin(op, left, right))
    }

    fn additive(&mut self) -> R<Expr> {
        let mut left = self.multiplicative()?;
        let mut n = 0;
        loop {
            let op = if self.is_punct("+") {
                BinOp::Add
            } else if self.is_punct("-") {
                BinOp::Sub
            } else if self.is_punct(".") {
                BinOp::Concat
            } else {
                return Ok(left);
            };
            self.chain(&mut n)?;
            self.advance();
            let right = self.multiplicative()?;
            left = bin(op, left, right);
        }
    }

    fn multiplicative(&mut self) -> R<Expr> {
        let mut left = self.unary()?;
        let mut n = 0;
        while self.is_punct("*") {
            self.chain(&mut n)?;
            self.advance();
            let right = self.unary()?;
            left = bin(BinOp::Mul, left, right);
        }
        Ok(left)
    }

    fn unary(&mut self) -> R<Expr> {
        if self.is_punct("-") {
            let span = self.advance();
            let e = self.nested(Self::unary)?;
            return Ok(Expr { kind: ExprKind::Neg(Box::new(e)), span });
        }
        self.postfix()
    }

    /// Consumes the method name after `->` (qualified names are rejected) and its optional argument list.
    fn method_args(&mut self, method: &str) -> R<Args> {
        if method.contains("::") {
            return self.err(self.span(), "qualified method names are not supported");
        }
        self.advance();
        if self.is_punct("(") {
            self.call_args()
        } else {
            Ok(Args::Positional(vec![]))
        }
    }

    fn postfix(&mut self) -> R<Expr> {
        let mut e = self.primary()?;
        let mut n = 0;
        while self.is_punct("->") {
            self.chain(&mut n)?;
            let arrow = self.advance();
            let span = e.span;
            match self.tok().clone() {
                Tok::Ident(method) => {
                    let args = self.method_args(&method)?;
                    e = Expr { kind: ExprKind::MethodCall { recv: Box::new(e), method, args }, span };
                }
                Tok::Punct("{") => {
                    self.advance();
                    let name = self.bare_key("field keys must be barewords")?;
                    self.expect_punct("}")?;
                    e = Expr { kind: ExprKind::Field { recv: Box::new(e), name }, span };
                }
                Tok::Punct("[") => return self.err(arrow, "array element access (`->[...]`) is not supported"),
                Tok::Punct("(") => return self.err(arrow, "calling code references is not supported"),
                Tok::Var(_) => return self.err(arrow, "dynamic method names are not supported"),
                _ => return self.unexpected("a method name"),
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> R<Expr> {
        let span = self.span();
        match self.tok().clone() {
            Tok::Int(n) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Int(n), span })
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Str(s), span })
            }
            Tok::Var(v) => {
                self.advance();
                Ok(Expr { kind: ExprKind::Var(v), span })
            }
            Tok::Punct("(") => {
                self.advance();
                let e = self.expr()?;
                if self.is_punct(",") {
                    return self.err(span, "list expressions (comma operator) are not supported");
                }
                self.expect_punct(")")?;
                Ok(e)
            }
            Tok::Punct("[") => {
                self.advance();
                let items = self.comma_list("]", Self::expr)?;
                Ok(Expr { kind: ExprKind::Array(items), span })
            }
            Tok::Punct("{") => {
                self.advance();
                let pairs = self.pairs()?;
                Ok(Expr { kind: ExprKind::Hash(pairs), span })
            }
            Tok::Punct("*") => self.err(span, "typeglobs are not supported"),
            Tok::Ident(w) => self.word(w, span),
            _ => self.unexpected("an expression"),
        }
    }

    /// Items separated by `,` (a trailing comma is fine), up to and including `close`.
    fn comma_list<T>(&mut self, close: &str, mut item: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        let mut out = Vec::new();
        while !self.is_punct(close) {
            out.push(item(self)?);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(close)?;
        Ok(out)
    }

    /// `key => expr, ...` up to and including the closing `}`.
    fn pairs(&mut self) -> R<Vec<Pair>> {
        self.comma_list("}", |p| {
            let span = p.span();
            let key = p.bare_key("hash keys must be barewords")?;
            p.expect_punct("=>")?;
            let value = p.expr()?;
            Ok(Pair { key, value, span })
        })
    }

    fn word(&mut self, w: String, span: Span) -> R<Expr> {
        if let Some(m) = banned(&w) {
            return self.err(span, m);
        }
        match w.as_str() {
            "sub" => return self.err(span, "anonymous subs and closures are not supported"),
            "die" => return self.err(span, "die can only be used as a statement"),
            "my" => return self.err(span, "my is only allowed as a statement"),
            "bless" => return self.bless(span),
            _ => {}
        }
        self.advance();
        if self.is_punct("(") {
            let args = self.call_args()?;
            return Ok(Expr { kind: ExprKind::Call { name: w, args }, span });
        }
        if self.eat_punct("->") {
            match self.tok().clone() {
                Tok::Ident(m) => {
                    let args = self.method_args(&m)?;
                    return Ok(Expr { kind: ExprKind::ClassCall { class: w, method: m, args }, span });
                }
                Tok::Var(_) => return self.err(self.span(), "dynamic method names are not supported"),
                _ => return self.unexpected("a method name"),
            }
        }
        if WORD_OPS.contains(&w.as_str()) {
            return self.err(span, format!("operator `{w}` is not supported"));
        }
        self.err(span, format!("bareword `{w}` is not supported"))
    }

    fn at_pair(&self) -> bool {
        matches!(self.tok(), Tok::Ident(_)) && matches!(self.tok_at(1), Tok::Punct("=>"))
    }

    fn call_args(&mut self) -> R<Args> {
        self.expect_punct("(")?;
        if self.at_pair() {
            return self.comma_list(")", |p| {
                let span = p.span();
                if !p.at_pair() {
                    return p.err(span, "cannot mix positional and named arguments");
                }
                let key = p.name("a key")?;
                p.advance(); // =>
                let value = p.expr()?;
                Ok(Pair { key, value, span })
            }).map(Args::Named);
        }
        self.comma_list(")", |p| {
            if p.at_pair() {
                return p.err(p.span(), "cannot mix positional and named arguments");
            }
            p.expr()
        }).map(Args::Positional)
    }

    fn bless(&mut self, span: Span) -> R<Expr> {
        self.advance(); // bless
        self.expect_punct("(")?;
        if !self.eat_punct("{") {
            return self.err(self.span(), "bless requires a hash literal as its first argument (re-blessing is not allowed)");
        }
        let fields = self.pairs()?;
        if !self.eat_punct(",") {
            return self.err(self.span(), "bless requires two arguments: `bless({...}, $class)`");
        }
        let target = self.expr()?;
        self.expect_punct(")")?;
        Ok(Expr { kind: ExprKind::Bless { fields, target: Box::new(target) }, span })
    }
}

fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    let span = l.span;
    Expr { kind: ExprKind::Binary(op, Box::new(l), Box::new(r)), span }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sexp(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => n.clone(),
            ExprKind::Var(v) => format!("${v}"),
            ExprKind::Str(_) => "str".into(),
            ExprKind::Neg(x) => format!("(neg {})", sexp(x)),
            ExprKind::Binary(op, l, r) => format!("({} {} {})", op.symbol(), sexp(l), sexp(r)),
            ExprKind::Call { name, .. } => format!("(call {name})"),
            ExprKind::ClassCall { class, method, .. } => format!("({class}->{method})"),
            ExprKind::MethodCall { recv, method, .. } => format!("({}->{method})", sexp(recv)),
            ExprKind::Field { recv, name } => format!("({}->{{{name}}})", sexp(recv)),
            ExprKind::Array(v) => format!("[{}]", v.len()),
            ExprKind::Hash(v) => format!("{{{}}}", v.len()),
            ExprKind::Bless { .. } => "bless".into(),
        }
    }

    fn init(src: &str) -> String {
        let f = parse("t.tpr", src, FileKind::Script).unwrap();
        match &f.items[0] {
            Item::Stmt(Stmt::My { init, .. }) => sexp(init),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_precedence() {
        assert_eq!(init("my Int $v = 1 + 2 * 3 - 4;"), "(- (+ 1 (* 2 3)) 4)");
        assert_eq!(init("my Any $v = -1 * 2 . \"a\";"), "(. (* (neg 1) 2) str)");
        assert_eq!(init("my Bool $v = 1 + 2 == 3;"), "(== (+ 1 2) 3)");
        assert_eq!(init("my Int $v = (1 + 2) * - -3;"), "(* (+ 1 2) (neg (neg 3)))");
    }

    #[test]
    fn parses_calls_and_chains() {
        assert_eq!(init("my Any $v = Point->new(x => 1)->move(x => 2)->x;"), "(((Point->new)->move)->x)");
        assert_eq!(init("my Any $v = $self->{at}->x();"), "(($self->{at})->x)");
        assert_eq!(init("my Any $v = Math::add(1, 2);"), "(call Math::add)");
    }

    #[test]
    fn bounds_nesting_and_chains() {
        let e = |src: String| parse("t.tpr", &src, FileKind::Script).unwrap_err().msg;
        assert_eq!(e(format!("my Int $v = {}1{};", "(".repeat(200), ")".repeat(200))), "nesting is too deep (limit 64)");
        assert_eq!(e(format!("my Int $v = 1{};", " + 1".repeat(300))), "expression is too long (limit 256)");
        assert!(parse("t.tpr", &format!("my Int $v = {}1{};", "(".repeat(63), ")".repeat(63)), FileKind::Script).is_ok());
    }

    #[test]
    fn parses_args_shapes() {
        let f = parse("t.tpr", "f(1, 2);\ng(a => 1, b => 2,);\nh();\n", FileKind::Script).unwrap();
        let shapes: Vec<String> = f
            .items
            .iter()
            .map(|i| match i {
                Item::Stmt(Stmt::Expr(Expr { kind: ExprKind::Call { args: Args::Positional(v), .. }, .. })) => format!("pos{}", v.len()),
                Item::Stmt(Stmt::Expr(Expr { kind: ExprKind::Call { args: Args::Named(v), .. }, .. })) => format!("named{}", v.len()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(shapes, vec!["pos2", "named2", "pos0"]);
    }

    #[test]
    fn parses_module_layout() {
        let f = parse(
            "Point.tpm",
            "package Point;\nuse Foo;\nfield x: Optional[Int|Str];\nsub new(Class $class, :Int $x) -> Point {\n    return bless({ x => $x }, $class);\n}\n",
            FileKind::Module,
        )
        .unwrap();
        assert_eq!(f.package.as_ref().unwrap().0, "Point");
        assert_eq!(f.uses[0].name, "Foo");
        match &f.items[0] {
            Item::Field(fl) => assert_eq!(fl.ty, Type::Optional(Box::new(Type::Union(vec![Type::Int, Type::Str])))),
            other => panic!("{other:?}"),
        }
        match &f.items[1] {
            Item::Sub(s) => {
                assert_eq!(s.params.len(), 2);
                assert!(!s.params[0].named && s.params[1].named);
                assert_eq!(s.ret, Type::Object("Point".into()));
                assert!(matches!(&s.body[0], Stmt::Return { value: Some(Expr { kind: ExprKind::Bless { .. }, .. }), .. }));
            }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn allows_trailing_comma_and_requires_separator() {
        assert!(parse("t.tpr", "my Any $a = [1, 2,];\nmy Any $h = { a => 1, };\nf(1, 2,);\n", FileKind::Script).is_ok());
        assert_eq!(parse("t.tpr", "my Any $a = [1 2];", FileKind::Script).unwrap_err().msg, "expected `]`, found `2`");
    }
}
