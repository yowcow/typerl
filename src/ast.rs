use crate::diag::Span;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Module,
    Script,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Int,
    Str,
    Bool,
    Any,
    Void,
    Class,
    ArrayRef(Box<Type>),
    HashRef(Box<Type>),
    Optional(Box<Type>),
    Union(Vec<Type>),
    /// A class name: typed (has a .tpm) or legacy Perl.
    Object(String),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Str => write!(f, "Str"),
            Type::Bool => write!(f, "Bool"),
            Type::Any => write!(f, "Any"),
            Type::Void => write!(f, "Void"),
            Type::Class => write!(f, "Class"),
            Type::ArrayRef(t) => write!(f, "ArrayRef[{t}]"),
            Type::HashRef(t) => write!(f, "HashRef[{t}]"),
            Type::Optional(t) => write!(f, "Optional[{t}]"),
            Type::Union(ts) => {
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        write!(f, "|")?;
                    }
                    write!(f, "{t}")?;
                }
                Ok(())
            }
            Type::Object(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Debug)]
pub struct File {
    pub kind: FileKind,
    pub package: Option<(String, Span)>,
    pub uses: Vec<Use>,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct Use {
    pub name: String,
}

#[derive(Debug)]
pub enum Item {
    Field(Field),
    Sub(Sub),
    Stmt(Stmt),
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug)]
pub struct Sub {
    pub name: String,
    /// All parameters as written, invocant (`$class` / `$self`) included.
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub named: bool,
    pub span: Span,
}

#[derive(Debug)]
pub enum Stmt {
    My {
        ty: Type,
        name: String,
        init: Expr,
        span: Span,
    },
    If {
        arms: Vec<(Expr, Vec<Stmt>)>,
        els: Option<Vec<Stmt>>,
    },
    Foreach {
        ty: Type,
        var: String,
        list: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Die {
        msg: Expr,
    },
    Expr(Expr),
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    /// Start of the expression (for binary expressions: start of the left operand).
    pub span: Span,
}

#[derive(Debug)]
pub enum ExprKind {
    Int(String),
    Str(StrLit),
    Var(String),
    Array(Vec<Expr>),
    Hash(Vec<Pair>),
    Neg(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `f(...)`, `Pkg::f(...)`, `to_int(...)`/`to_str(...)`/`to_bool(...)`.
    Call {
        name: String,
        args: Args,
    },
    /// `Pkg->m(...)`
    ClassCall {
        class: String,
        method: String,
        args: Args,
    },
    /// `$e->m(...)` / `$e->m`
    MethodCall {
        recv: Box<Expr>,
        method: String,
        args: Args,
    },
    /// `$e->{name}`
    Field {
        recv: Box<Expr>,
        name: String,
    },
    /// `bless({ ... }, target)`
    Bless {
        fields: Vec<Pair>,
        target: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrLit {
    /// Raw source text including the quotes, e.g. `'it\'s'`.
    Single(String),
    Double(Vec<StrPart>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    /// Raw source text between the quotes, escapes kept as written (`\n` stays two chars).
    Lit(String),
    /// `$name`, with the span of the `$`.
    Var(String, Span),
}

#[derive(Debug)]
pub struct Pair {
    pub key: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub enum Args {
    Positional(Vec<Expr>),
    Named(Vec<Pair>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Concat,
    NumEq,
    StrEq,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Concat => ".",
            BinOp::NumEq => "==",
            BinOp::StrEq => "eq",
        }
    }

    /// Perl 5 precedence, higher binds tighter.
    pub fn prec(self) -> u8 {
        match self {
            BinOp::Mul => 3,
            BinOp::Add | BinOp::Sub | BinOp::Concat => 2,
            BinOp::NumEq | BinOp::StrEq => 1,
        }
    }
}

/// True when every path through `stmts` ends in `return` or `die`.
/// `any` (not just the last statement) suffices: a terminating `return`/`die` or fully-terminating
/// `if`/`else` never falls through, so trailing code is dead (`foreach` is conservatively
/// non-terminating; there is no `break`/`last`/`while`).
pub fn terminates(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Return { .. } | Stmt::Die { .. } => true,
        Stmt::If { arms, els: Some(e) } => arms.iter().all(|(_, b)| terminates(b)) && terminates(e),
        _ => false,
    })
}

/// Return type of a built-in conversion, or None for other function names.
pub fn conversion_type(name: &str) -> Option<Type> {
    match name {
        "to_int" => Some(Type::Int),
        "to_str" => Some(Type::Str),
        "to_bool" => Some(Type::Bool),
        _ => None,
    }
}
