use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::parser;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubKind {
    Function,
    Method,
    Constructor,
}

#[derive(Debug, Clone)]
pub struct SubSig {
    pub name: String,
    pub kind: SubKind,
    /// Parameters after the invocant.
    pub params: Vec<Param>,
    pub named: bool,
    pub ret: Type,
}

#[derive(Debug)]
pub struct ModuleSig {
    pub package: String,
    pub subs: HashMap<String, SubSig>,
    pub fields: Vec<(String, Type)>,
}

pub const RESERVED: &[&str] = &[
    "Int", "Str", "Bool", "Any", "Void", "Class", "ArrayRef", "HashRef", "Optional", "Maybe", "InstanceOf", "to_int",
    "to_str", "to_bool", "BEGIN", "CHECK", "INIT", "END", "UNITCHECK", "AUTOLOAD", "DESTROY", "import", "unimport",
];

/// Perl 5.38 built-in function names and keywords (from B::Keywords); an unqualified call
/// with one of these names would not reach a user sub.
pub const PERL_BUILTINS: &[&str] = &[
    "__CLASS__", "__DATA__", "__END__", "__FILE__", "__LINE__", "__PACKAGE__", "__SUB__",
    "abs", "accept", "alarm", "and", "atan2", "bind", "binmode", "bless", "break", "caller", "catch", "chdir", "chmod",
    "chomp", "chop", "chown", "chr", "chroot", "class", "close", "closedir", "cmp", "connect", "continue", "cos",
    "crypt", "dbmclose", "dbmopen", "default", "defer", "defined", "delete", "die", "do", "dump", "each", "else",
    "elsif", "endgrent", "endhostent", "endnetent", "endprotoent", "endpwent", "endservent", "eof", "eq", "eval",
    "evalbytes", "exec", "exists", "exit", "exp", "fc", "fcntl", "field", "fileno", "finally", "flock", "for",
    "foreach", "fork", "format", "formline", "ge", "getc", "getgrent", "getgrgid", "getgrnam", "gethostbyaddr",
    "gethostbyname", "gethostent", "getlogin", "getnetbyaddr", "getnetbyname", "getnetent", "getpeername",
    "getpgrp", "getppid", "getpriority", "getprotobyname", "getprotobynumber", "getprotoent", "getpwent",
    "getpwnam", "getpwuid", "getservbyname", "getservbyport", "getservent", "getsockname", "getsockopt", "given",
    "glob", "gmtime", "goto", "grep", "gt", "hex", "if", "index", "int", "ioctl", "isa", "join", "keys", "kill",
    "last", "lc", "lcfirst", "le", "length", "link", "listen", "local", "localtime", "lock", "log", "lstat", "lt",
    "m", "map", "method", "mkdir", "msgctl", "msgget", "msgrcv", "msgsnd", "my", "ne", "next", "no", "not", "oct",
    "open", "opendir", "or", "ord", "our", "pack", "package", "pipe", "pop", "pos", "print", "printf", "prototype",
    "push", "q", "qq", "qr", "quotemeta", "qw", "qx", "rand", "read", "readdir", "readline", "readlink",
    "readpipe", "recv", "redo", "ref", "rename", "require", "reset", "return", "reverse", "rewinddir", "rindex",
    "rmdir", "s", "say", "scalar", "seek", "seekdir", "select", "semctl", "semget", "semop", "send", "setgrent",
    "sethostent", "setnetent", "setpgrp", "setpriority", "setprotoent", "setpwent", "setservent", "setsockopt",
    "shift", "shmctl", "shmget", "shmread", "shmwrite", "shutdown", "sin", "sleep", "socket", "socketpair", "sort",
    "splice", "split", "sprintf", "sqrt", "srand", "stat", "state", "study", "sub", "substr", "symlink",
    "syscall", "sysopen", "sysread", "sysseek", "system", "syswrite", "tell", "telldir", "tie", "tied", "time",
    "times", "tr", "truncate", "try", "uc", "ucfirst", "umask", "undef", "unless", "unlink", "unpack", "unshift",
    "untie", "until", "use", "utime", "values", "vec", "wait", "waitpid", "wantarray", "warn", "when", "while",
    "write", "x", "xor", "y",
];

pub fn expected_package(path: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for c in Path::new(path).with_extension("").components() {
        match c {
            Component::CurDir => {}
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            _ => return Err("module paths must be relative to the current directory".into()),
        }
    }
    Ok(parts.join("::"))
}

pub fn module_path(package: &str) -> PathBuf {
    PathBuf::from(format!("{}.tpm", package.replace("::", "/")))
}

pub fn check_package(file: &File, path: &str) -> Result<(), Diag> {
    let (name, span) = file.package.as_ref().expect("modules always have a package");
    let expected = expected_package(path).map_err(|m| Diag::new(path, *span, m))?;
    if *name != expected {
        return Err(Diag::new(path, *span, format!("package `{name}` does not match the file name (expected `{expected}`)")));
    }
    Ok(())
}

/// Validates a written type. `known` = class names usable in this file (own package + uses).
pub fn check_type(ty: &Type, known: &HashSet<String>, allow_void: bool) -> Result<(), String> {
    match ty {
        Type::Void if allow_void => Ok(()),
        Type::Void => Err("Void is only allowed as a return type".into()),
        Type::Class => Err("Class is only allowed as the type of a constructor's first parameter `$class`".into()),
        Type::Object(n) if !known.contains(n) => Err(format!("unknown type `{n}`")),
        Type::ArrayRef(t) | Type::HashRef(t) | Type::Optional(t) => check_type(t, known, false),
        Type::Union(ts) => ts.iter().try_for_each(|t| check_type(t, known, false)),
        _ => Ok(()),
    }
}

pub fn known_names(file: &File) -> HashSet<String> {
    let mut known: HashSet<String> = file.uses.iter().map(|u| u.name.clone()).collect();
    if let Some((p, _)) = &file.package {
        known.insert(p.clone());
    }
    known
}

pub fn module_sig(file: &File, path: &str) -> Result<ModuleSig, Diag> {
    let package = file.package.as_ref().map_or_else(|| "main".to_string(), |p| p.0.clone());
    let known = known_names(file);
    let mut sig = ModuleSig { package: package.clone(), subs: HashMap::new(), fields: Vec::new() };
    for item in &file.items {
        match item {
            Item::Field(f) => {
                if sig.fields.iter().any(|(n, _)| *n == f.name) {
                    return Err(Diag::new(path, f.span, format!("duplicate field `{}`", f.name)));
                }
                check_type(&f.ty, &known, false).map_err(|m| Diag::new(path, f.span, m))?;
                sig.fields.push((f.name.clone(), f.ty.clone()));
            }
            Item::Sub(s) => {
                let ss = sub_sig(s, file.kind, &package, &known).map_err(|(sp, m)| Diag::new(path, sp, m))?;
                if sig.subs.contains_key(&s.name) {
                    return Err(Diag::new(path, s.span, format!("duplicate sub `{}`", s.name)));
                }
                sig.subs.insert(s.name.clone(), ss);
            }
            Item::Stmt(_) => {}
        }
    }
    Ok(sig)
}

fn sub_sig(s: &Sub, kind: FileKind, package: &str, known: &HashSet<String>) -> Result<SubSig, (Span, String)> {
    if RESERVED.contains(&s.name.as_str()) {
        return Err((s.span, format!("`{}` is a reserved name", s.name)));
    }
    let mut params = s.params.clone();
    let sub_kind = match params.first().map(|p| p.name.as_str()) {
        Some("class") => SubKind::Constructor,
        Some("self") => SubKind::Method,
        _ => SubKind::Function,
    };
    if sub_kind != SubKind::Function {
        let inv = params.remove(0);
        if kind == FileKind::Script {
            return Err((inv.span, "constructors and methods are only allowed in modules (.tpm)".into()));
        }
        if inv.named {
            return Err((inv.span, format!("invocant `${}` must be positional", inv.name)));
        }
        let own = Type::Object(package.to_string());
        if sub_kind == SubKind::Constructor && inv.ty != Type::Class {
            return Err((inv.span, "constructor invocant `$class` must have type Class".into()));
        }
        if sub_kind == SubKind::Method && inv.ty != own {
            return Err((inv.span, format!("method invocant `$self` must have type {package}")));
        }
    }
    if sub_kind == SubKind::Function && PERL_BUILTINS.contains(&s.name.as_str()) {
        return Err((s.span, format!("`{}` is a Perl built-in; a function with this name cannot be called safely", s.name)));
    }
    let mut seen = HashSet::new();
    for p in &params {
        if p.name == "class" || p.name == "self" {
            return Err((p.span, format!("`${}` must be the first parameter", p.name)));
        }
        if !seen.insert(p.name.clone()) {
            return Err((p.span, format!("duplicate parameter `${}`", p.name)));
        }
        check_type(&p.ty, known, false).map_err(|m| (p.span, m))?;
    }
    let named = params.first().map_or(false, |p| p.named);
    if params.iter().any(|p| p.named != named) {
        return Err((s.span, "positional and named parameters cannot be mixed".into()));
    }
    if sub_kind == SubKind::Constructor {
        if !params.is_empty() && !named {
            return Err((s.span, "constructor parameters after `$class` must be named (`:Type $name`)".into()));
        }
        if s.ret != Type::Object(package.to_string()) {
            return Err((s.span, format!("constructor must return {package}")));
        }
    }
    check_type(&s.ret, known, true).map_err(|m| (s.span, m))?;
    Ok(SubSig { name: s.name.clone(), kind: sub_kind, params, named, ret: s.ret.clone() })
}

/// Signatures of `use`d typed modules, read from `<cwd>/<Pkg path>.tpm`.
#[derive(Default)]
pub struct Loader {
    cache: HashMap<String, Option<Rc<ModuleSig>>>,
}

impl Loader {
    /// `Some` for a typed module, `None` for legacy Perl (no .tpm).
    pub fn get(&mut self, package: &str) -> Result<Option<Rc<ModuleSig>>, Diag> {
        if let Some(hit) = self.cache.get(package) {
            return Ok(hit.clone());
        }
        let path = module_path(package);
        let sig = if path.is_file() {
            let p = path.to_string_lossy().into_owned();
            let src = std::fs::read_to_string(&path)
                .map_err(|e| Diag::new(&p, Span::START, format!("cannot read file: {e}")))?;
            let file = parser::parse(&p, &src, FileKind::Module)?;
            check_package(&file, &p)?;
            Some(Rc::new(module_sig(&file, &p)?))
        } else {
            None
        };
        self.cache.insert(package.to_string(), sig.clone());
        Ok(sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_package_from_path() {
        assert_eq!(expected_package("Point.tpm").unwrap(), "Point");
        assert_eq!(expected_package("./Point/Label.tpm").unwrap(), "Point::Label");
        assert!(expected_package("/abs/Point.tpm").is_err());
        assert!(expected_package("../x/Point.tpm").is_err());
    }

    #[test]
    fn module_path_from_package() {
        assert_eq!(module_path("Point::Label"), PathBuf::from("Point/Label.tpm"));
    }

    #[test]
    fn check_type_rules() {
        let known: HashSet<String> = ["Point".to_string()].into();
        assert!(check_type(&Type::Object("Point".into()), &known, false).is_ok());
        assert_eq!(check_type(&Type::Object("Foo".into()), &known, false).unwrap_err(), "unknown type `Foo`");
        assert!(check_type(&Type::Void, &known, true).is_ok());
        assert!(check_type(&Type::ArrayRef(Box::new(Type::Void)), &known, true).is_err());
        assert!(check_type(&Type::Class, &known, false).is_err());
    }
}
