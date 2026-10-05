use crate::ast::{StrLit, StrPart};
use crate::diag::{Diag, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Var(String),
    Int(String),
    Str(StrLit),
    Punct(&'static str),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

const PUNCT: &[&str] = &["->", "=>", "==", "(", ")", "[", "]", "{", "}", ",", ";", ":", "+", "-", "*", ".", "=", "|"];
const BAD_OPS: &[&str] = &[
    "<=>", "**=", "||=", "&&=", "//=", "...", "<<", ">>", "**", "++", "--", "+=", "-=", "*=", "/=", ".=", "%=",
    "|=", "&=", "^=", "||", "&&", "//", "..", "!=", "<=", ">=", "=~", "!~", "~~", "<", ">", "!", "~", "^", "?",
    "/", "%", "&",
];
const INTERP_MSG: &str = "only simple `$name` interpolation of Str variables is supported";

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

struct Lexer<'a> {
    file: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
}

pub fn lex(file: &str, src: &str) -> Result<Vec<Token>, Diag> {
    let mut lx = Lexer { file, chars: src.chars().collect(), pos: 0, line: 1, col: 1 };
    let mut toks = Vec::new();
    while let Some(c) = lx.peek(0) {
        let span = lx.span();
        let at_line_start = lx.col == 1;
        if c.is_whitespace() {
            lx.bump();
            continue;
        }
        if c == '#' {
            while lx.peek(0).is_some_and(|c| c != '\n') {
                lx.bump();
            }
            continue;
        }
        if c == '=' && at_line_start && lx.peek(1).is_some_and(|n| n.is_ascii_alphabetic()) {
            return lx.err(span, "POD is not supported");
        }
        let sigil_next = lx.peek(1).is_some_and(|n| is_ident_start(n) || n == '$' || n == '{');
        let tok = if is_ident_start(c) {
            lx.ident()?
        } else if c.is_ascii_digit() {
            lx.number()?
        } else if c == '$' {
            lx.var()?
        } else if c == '\'' {
            lx.single()?
        } else if c == '"' {
            lx.double()?
        } else if c == '@' {
            return lx.err(span, "array variables are not supported (use ArrayRef)");
        } else if c == '%' && sigil_next {
            return lx.err(span, "hash variables are not supported (use HashRef)");
        } else if c == '&' && sigil_next {
            return lx.err(span, "`&` (subroutine sigil) is not supported");
        } else if c == '\\' {
            return lx.err(span, "references (`\\`) are not supported");
        } else if c == '`' {
            return lx.err(span, "backticks are not supported");
        } else {
            lx.punct()?
        };
        toks.push(Token { tok, span });
    }
    toks.push(Token { tok: Tok::Eof, span: lx.span() });
    Ok(toks)
}

impl Lexer<'_> {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn span(&self) -> Span {
        Span { line: self.line, col: self.col }
    }

    fn err<T>(&self, span: Span, msg: impl Into<String>) -> Result<T, Diag> {
        Err(Diag::new(self.file, span, msg))
    }

    fn take_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0).filter(|c| pred(*c)) {
            s.push(c);
            self.bump();
        }
        s
    }

    fn word(&mut self) -> String {
        self.take_while(is_ident_char)
    }

    fn ident(&mut self) -> Result<Tok, Diag> {
        let mut s = self.word();
        while self.peek(0) == Some(':') && self.peek(1) == Some(':') {
            if !self.peek(2).is_some_and(is_ident_start) {
                return self.err(self.span(), "operator `::` is not supported");
            }
            self.bump();
            self.bump();
            s.push_str("::");
            s.push_str(&self.word());
        }
        if self.peek(0) == Some('\'') && self.peek(1).is_some_and(is_ident_start) {
            return self.err(self.span(), "`'` as a package separator is not supported");
        }
        Ok(Tok::Ident(s))
    }

    fn number(&mut self) -> Result<Tok, Diag> {
        let span = self.span();
        let s = self.take_while(|c| c.is_ascii_digit());
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            return self.err(span, "floating-point numbers are not supported");
        }
        if self.peek(0).is_some_and(is_ident_char) {
            return self.err(span, "invalid number literal");
        }
        if s.len() > 1 && s.starts_with('0') {
            return self.err(span, "leading zeros are not supported (Perl reads them as octal)");
        }
        Ok(Tok::Int(s))
    }

    fn var(&mut self) -> Result<Tok, Diag> {
        let span = self.span();
        self.bump(); // $
        match self.peek(0) {
            Some('$') | Some('{') => return self.err(span, "symbolic references and dereferencing are not supported"),
            Some(c) if is_ident_start(c) => {}
            _ => return self.err(span, "special variables are not supported"),
        }
        let name = self.word();
        if name == "_" {
            return self.err(span, "special variables are not supported");
        }
        if self.peek(0) == Some(':') && self.peek(1) == Some(':') {
            return self.err(span, "package variables are not supported");
        }
        Ok(Tok::Var(name))
    }

    fn single(&mut self) -> Result<Tok, Diag> {
        let span = self.span();
        let mut raw = String::new();
        raw.push(self.bump().unwrap());
        loop {
            let here = self.span();
            match self.bump() {
                None => return self.err(span, "unterminated string"),
                Some('\\') => match self.bump() {
                    Some(c @ ('\\' | '\'')) => {
                        raw.push('\\');
                        raw.push(c);
                    }
                    Some(c) => return self.err(here, format!("unsupported escape `\\{c}`")),
                    None => return self.err(span, "unterminated string"),
                },
                Some('\'') => {
                    raw.push('\'');
                    return Ok(Tok::Str(StrLit::Single(raw)));
                }
                Some(c) => raw.push(c),
            }
        }
    }

    fn double(&mut self) -> Result<Tok, Diag> {
        let span = self.span();
        self.bump(); // "
        let mut parts = Vec::new();
        let mut lit = String::new();
        loop {
            let here = self.span();
            match self.bump() {
                None => return self.err(span, "unterminated string"),
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some(c @ ('n' | 't' | '\\' | '"' | '$' | '@')) => {
                        lit.push('\\');
                        lit.push(c);
                    }
                    Some(c) => return self.err(here, format!("unsupported escape `\\{c}`")),
                    None => return self.err(span, "unterminated string"),
                },
                Some('@') => return self.err(here, "`@` must be escaped as `\\@` in double-quoted strings"),
                Some('$') => {
                    if self.peek(0) == Some('{') {
                        return self.err(here, INTERP_MSG);
                    }
                    if !self.peek(0).is_some_and(is_ident_start) {
                        return self.err(here, "a literal `$` must be escaped as `\\$`");
                    }
                    let name = self.word();
                    let (a, b, c) = (self.peek(0), self.peek(1), self.peek(2));
                    let complex = name == "_"
                        || matches!(a, Some('[') | Some('{'))
                        || (a == Some(':') && b == Some(':'))
                        || (a == Some('-') && b == Some('>') && matches!(c, Some('[') | Some('{')))
                        || (a == Some('\'') && b.is_some_and(is_ident_start));
                    if complex {
                        return self.err(here, INTERP_MSG);
                    }
                    if !lit.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(StrPart::Var(name, here));
                }
                Some(c) => lit.push(c),
            }
        }
        if !lit.is_empty() {
            parts.push(StrPart::Lit(lit));
        }
        Ok(Tok::Str(StrLit::Double(parts)))
    }

    fn punct(&mut self) -> Result<Tok, Diag> {
        let span = self.span();
        for len in (1..=3).rev() {
            if self.pos + len > self.chars.len() {
                continue;
            }
            let s: String = self.chars[self.pos..self.pos + len].iter().collect();
            if let Some(op) = BAD_OPS.iter().find(|o| **o == s) {
                return self.err(span, format!("operator `{op}` is not supported"));
            }
            if let Some(p) = PUNCT.iter().find(|p| **p == s) {
                for _ in 0..len {
                    self.bump();
                }
                return Ok(Tok::Punct(p));
            }
        }
        self.err(span, format!("unexpected character `{}`", self.peek(0).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex("t", src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn err(src: &str) -> String {
        lex("t", src).unwrap_err().msg
    }

    #[test]
    fn lexes_tokens_with_spans() {
        let t = lex("t", "my Int $x = 1;\n  foo").unwrap();
        let got: Vec<(Tok, u32, u32)> = t.into_iter().map(|t| (t.tok, t.span.line, t.span.col)).collect();
        assert_eq!(
            got,
            vec![
                (Tok::Ident("my".into()), 1, 1),
                (Tok::Ident("Int".into()), 1, 4),
                (Tok::Var("x".into()), 1, 8),
                (Tok::Punct("="), 1, 11),
                (Tok::Int("1".into()), 1, 13),
                (Tok::Punct(";"), 1, 14),
                (Tok::Ident("foo".into()), 2, 3),
                (Tok::Eof, 2, 6),
            ]
        );
    }

    #[test]
    fn lexes_qualified_names_and_arrows() {
        assert_eq!(
            toks("Point::Label->new(x => 1)"),
            vec![
                Tok::Ident("Point::Label".into()),
                Tok::Punct("->"),
                Tok::Ident("new".into()),
                Tok::Punct("("),
                Tok::Ident("x".into()),
                Tok::Punct("=>"),
                Tok::Int("1".into()),
                Tok::Punct(")"),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn lexes_strings() {
        assert_eq!(toks("'it\\'s'"), vec![Tok::Str(StrLit::Single("'it\\'s'".into())), Tok::Eof]);
        assert_eq!(
            toks("\"hi $name!\\n\""),
            vec![
                Tok::Str(StrLit::Double(vec![
                    StrPart::Lit("hi ".into()),
                    StrPart::Var("name".into(), Span { line: 1, col: 5 }),
                    StrPart::Lit("!\\n".into()),
                ])),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn skips_comments() {
        assert_eq!(toks("1 # two\n3"), vec![Tok::Int("1".into()), Tok::Int("3".into()), Tok::Eof]);
    }

    #[test]
    fn rejects_unsupported_operators() {
        for op in ["<", "!=", "++", "+=", "**", "&&", "||", "//", "..", "=~", "<=>", "?", "%", "/"] {
            assert_eq!(err(&format!("1 {op} 2")), format!("operator `{op}` is not supported"));
        }
    }

    #[test]
    fn rejects_bad_numbers() {
        assert!(err("010").starts_with("leading zeros"));
        assert!(err("1.5").starts_with("floating-point"));
        assert_eq!(err("0x1"), "invalid number literal");
        assert_eq!(err("1_0"), "invalid number literal");
    }

    #[test]
    fn rejects_sigils_and_specials() {
        assert!(err("@a").starts_with("array variables"));
        assert!(err("%h").starts_with("hash variables"));
        assert!(err("&f").starts_with("`&`"));
        assert!(err("\\$x").starts_with("references"));
        assert!(err("$$x").starts_with("symbolic references"));
        assert!(err("${x}").starts_with("symbolic references"));
        assert_eq!(err("$_"), "special variables are not supported");
        assert_eq!(err("$0"), "special variables are not supported");
        assert_eq!(err("$Foo::x"), "package variables are not supported");
        assert_eq!(err("=pod\n"), "POD is not supported");
    }

    #[test]
    fn rejects_complex_interpolation() {
        for s in ["\"${x}\"", "\"$x->{a}\"", "\"$x->[0]\"", "\"$x[0]\"", "\"$x{a}\"", "\"$x::y\"", "\"$x's\""] {
            assert_eq!(err(s), "only simple `$name` interpolation of Str variables is supported", "{s}");
        }
        assert!(err("\"@x\"").starts_with("`@` must be escaped"));
        assert!(err("\"$5\"").starts_with("a literal `$`"));
        assert_eq!(err("\"\\q\""), "unsupported escape `\\q`");
        assert_eq!(err("\"abc"), "unterminated string");
    }
}
