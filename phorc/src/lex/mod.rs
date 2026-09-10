// Phorc — Phorensic Bootstrap Compiler
// Lexer / Tokenizer
//
// Implements the lexical specification from PHORENSIC_LANGUAGE.md §2

use std::fmt;

/// Source location: line:column
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
}

impl Loc {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    pub fn zero() -> Self {
        Self { line: 0, col: 0 }
    }
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Diagnostic code — every message has a permanent documented code
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diag {
    pub code: &'static str,
    pub loc: Loc,
    pub message: String,
}

impl Diag {
    pub fn new(code: &'static str, loc: Loc, message: impl Into<String>) -> Self {
        Self {
            code,
            loc,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: E{}: {}",
            self.loc.line, self.loc.col, self.code, self.message
        )
    }
}

/// Token kinds for Phorensic
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(u64),
    Float(f64),
    StrLit(String),
    Char(char),
    Byte(u8),
    Bool(bool),
    Ident(String),
    ResidualLit(String),

    // Keywords
    Fn,
    Let,
    Mut,
    In,
    Cap,
    Effect,
    Machine,
    Court,
    Oracle,
    Trust,
    Trusted,
    Residual,
    Sealed,
    Dialect,
    Cage,
    Bound,
    Loop,
    For,
    While,
    If,
    Else,
    Match,
    Return,
    Break,
    Continue,
    Yield,
    Spawn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    Type,
    Const,
    Static,
    Import,
    Export,
    Pub,
    Use,
    As,
    Ref,
    Package,
    Generation,
    Profile,
    True,
    False,
    Void,
    Never,
    // Type keywords
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    BoolTy,
    CharTy,
    F32,
    F64,
    Usize,
    Isize,
    Array,
    RingBuf,
    Slice,
    StrTy,
    CapTy,
    Handle,
    Result,
    Option,
    Provenance,
    Receipt,
    TrustState,
    ResidualTy,
    // Effect keywords
    NoStd,
    NoAlloc,
    NoUnsafe,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Exclaim,
    Lt,
    Gt,
    Eq,
    Colon,
    Semi,
    Comma,
    Dot,
    Arrow,
    FatArrow,
    Hash,
    At,
    Question,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    /// `<<` (distinct from `<`; previously collapsed into `Lt`)
    Shl,
    /// `>>` (distinct from `>`; previously collapsed into `Gt`)
    Shr,
    EqEq,
    Neq,
    Le,
    Ge,
    AndAnd,
    OrOr,
    DotDot,
    DotDotEq,
    DoubleColon,
    DoubleArrow,
    RArrow, // -> (thin arrow)

    // Special
    Comment,
    DocComment,
    Newline,
    Eof,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub loc: Loc,
}

impl Token {
    pub fn new(kind: TokenKind, loc: Loc) -> Self {
        Self { kind, loc }
    }
}

/// Lexer — produces tokens from Phorensic source text
pub struct Lexer {
    src: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    diags: Vec<Diag>,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Self {
            src: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            diags: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> &[Diag] {
        &self.diags
    }

    fn loc(&self) -> Loc {
        Loc::new(self.line, self.col)
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.src.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.src.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.bump();
                }
                _ => break,
            }
        }
    }

    fn read_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if !pred(c) {
                break;
            }
            s.push(c);
            self.bump();
        }
        s
    }

    fn read_number(&mut self, first: char) -> TokenKind {
        let loc = self.loc();
        let mut s = String::new();
        s.push(first);

        // Hex, binary, octal
        if first == '0' {
            match self.peek() {
                Some('x' | 'X') => {
                    s.push(self.bump().unwrap());
                    s.push_str(&self.read_while(|c| c.is_ascii_hexdigit() || c == '_'));
                    let clean: String = s.chars().filter(|c| *c != '_').collect();
                    if let Ok(v) = u64::from_str_radix(&clean[2..], 16) {
                        return TokenKind::Int(v);
                    }
                    // fallthrough
                }
                Some('b' | 'B') => {
                    s.push(self.bump().unwrap());
                    s.push_str(&self.read_while(|c| c == '0' || c == '1' || c == '_'));
                    let clean: String = s.chars().filter(|c| *c != '_').collect();
                    if let Ok(v) = u64::from_str_radix(&clean[2..], 2) {
                        return TokenKind::Int(v);
                    }
                }
                _ => {}
            }
        }

        // Decimal or float
        s.push_str(&self.read_while(|c| c.is_ascii_digit() || c == '_'));

        if self.peek() == Some('.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
            s.push(self.bump().unwrap());
            s.push_str(&self.read_while(|c| c.is_ascii_digit() || c == '_'));
            let clean: String = s.chars().filter(|c| *c != '_').collect();
            if let Ok(v) = clean.parse::<f64>() {
                return TokenKind::Float(v);
            }
        }

        let clean: String = s.chars().filter(|c| *c != '_').collect();
        if let Ok(v) = clean.parse::<u64>() {
            TokenKind::Int(v)
        } else {
            self.diags.push(Diag::new(
                "E0001",
                loc,
                format!("invalid numeric literal: {}", s),
            ));
            TokenKind::Error(format!("invalid number: {}", s))
        }
    }

    fn read_string(&mut self) -> TokenKind {
        let loc = self.loc();
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('x') => {
                        let hex: String = self.read_while(|c| c.is_ascii_hexdigit());
                        if let Ok(v) = u8::from_str_radix(&hex, 16) {
                            s.push(v as char);
                        }
                    }
                    Some(c) => {
                        s.push('\\');
                        s.push(c);
                    }
                    None => {
                        self.diags
                            .push(Diag::new("E0002", loc, "unterminated string literal"));
                        return TokenKind::Error("unterminated string".to_string());
                    }
                },
                Some(c) => s.push(c),
                None => {
                    self.diags
                        .push(Diag::new("E0002", loc, "unterminated string literal"));
                    return TokenKind::Error("unterminated string".to_string());
                }
            }
        }
        TokenKind::StrLit(s)
    }

    fn read_comment(&mut self) -> TokenKind {
        // Line comment — skip to end of line
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
        TokenKind::Comment
    }

    fn read_block_comment(&mut self) -> TokenKind {
        let mut depth = 1;
        while depth > 0 {
            match self.bump() {
                Some('/') if self.peek() == Some('*') => {
                    depth += 1;
                }
                Some('*') if self.peek() == Some('/') => {
                    self.bump();
                    depth -= 1;
                }
                Some(_) => {}
                None => break,
            }
        }
        TokenKind::Comment
    }

    fn ident_or_keyword(s: &str) -> TokenKind {
        match s {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "cap" => TokenKind::Cap,
            "effect" => TokenKind::Effect,
            "machine" => TokenKind::Machine,
            "court" => TokenKind::Court,
            "oracle" => TokenKind::Oracle,
            "trust" => TokenKind::Trust,
            "trusted" => TokenKind::Trusted,
            "in" => TokenKind::In,
            "residual" => TokenKind::Residual,
            "sealed" => TokenKind::Sealed,
            "dialect" => TokenKind::Dialect,
            "cage" => TokenKind::Cage,
            "bound" => TokenKind::Bound,
            "loop" => TokenKind::Loop,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "yield" => TokenKind::Yield,
            "spawn" => TokenKind::Spawn,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "union" => TokenKind::Union,
            "trait" => TokenKind::Trait,
            "impl" => TokenKind::Impl,
            "type" => TokenKind::Type,
            "const" => TokenKind::Const,
            "static" => TokenKind::Static,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "package" => TokenKind::Package,
            "generation" => TokenKind::Generation,
            "profile" => TokenKind::Profile,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "void" => TokenKind::Void,
            "never" => TokenKind::Never,
            "pub" => TokenKind::Pub,
            "use" => TokenKind::Use,
            "as" => TokenKind::As,
            "ref" => TokenKind::Ref,
            "u8" => TokenKind::U8,
            "u16" => TokenKind::U16,
            "u32" => TokenKind::U32,
            "u64" => TokenKind::U64,
            "i8" => TokenKind::I8,
            "i16" => TokenKind::I16,
            "i32" => TokenKind::I32,
            "i64" => TokenKind::I64,
            "bool" => TokenKind::BoolTy,
            "char" => TokenKind::CharTy,
            "f32" => TokenKind::F32,
            "f64" => TokenKind::F64,
            "usize" => TokenKind::Usize,
            "isize" => TokenKind::Isize,
            "Array" => TokenKind::Array,
            "RingBuf" => TokenKind::RingBuf,
            "Slice" => TokenKind::Slice,
            "Str" => TokenKind::StrTy,
            "Result" => TokenKind::Result,
            "Option" => TokenKind::Option,
            "Handle" => TokenKind::Handle,
            "Provenance" => TokenKind::Provenance,
            "Receipt" => TokenKind::Receipt,
            "TrustState" => TokenKind::TrustState,
            "no_std" => TokenKind::NoStd,
            "no_alloc" => TokenKind::NoAlloc,
            "no_unsafe" => TokenKind::NoUnsafe,
            _ => TokenKind::Ident(s.to_string()),
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let loc = self.loc();
        let c = match self.bump() {
            Some(c) => c,
            None => return Token::new(TokenKind::Eof, loc),
        };

        let kind = match c {
            // Line comments
            '/' if self.peek() == Some('/') => {
                self.read_comment();
                return self.next_token();
            }
            // Block comments
            '/' if self.peek() == Some('*') => {
                self.bump(); // consume '*'
                self.read_block_comment();
                return self.next_token();
            }
            // Doc comments
            '/' if self.peek() == Some('/') && self.peek_at(1) == Some('/') => {
                // We already consumed /, check if next two are //
                // Actually: we consumed '/', peek is '/', peek_at(1) is '/'
                // But let's handle this differently
                // Let's redo: we consumed '/', but for doc comment we need "///"
                // We'll handle it after the fact
                let mut s = String::from("//");
                s.push(self.bump().unwrap()); // second '/'
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    s.push(self.bump().unwrap());
                }
                TokenKind::DocComment
            }

            // Numbers
            '0'..='9' => return Token::new(self.read_number(c), loc),

            // Strings
            '"' => return Token::new(self.read_string(), loc),

            // Characters
            '\'' => {
                let ch = self.bump().unwrap_or('\0');
                if self.bump() != Some('\'') {
                    self.diags
                        .push(Diag::new("E0003", loc, "unterminated char literal"));
                    return Token::new(TokenKind::Error("unterminated char".to_string()), loc);
                }
                TokenKind::Char(ch)
            }

            // Operators and punctuation
            '+' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::MinusEq
                } else if self.peek() == Some('>') {
                    self.bump();
                    TokenKind::RArrow
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            '&' => {
                if self.peek() == Some('&') {
                    self.bump();
                    TokenKind::AndAnd
                } else if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::AmpEq
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.bump();
                    TokenKind::OrOr
                } else if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::PipeEq
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::CaretEq
                } else {
                    TokenKind::Caret
                }
            }
            '~' => TokenKind::Tilde,
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::Neq
                } else {
                    TokenKind::Exclaim
                }
            }
            '<' => {
                if self.peek() == Some('<') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::ShlEq
                    } else {
                        TokenKind::Shl
                    }
                } else if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('>') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::ShrEq
                    } else {
                        TokenKind::Shr
                    }
                } else if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            '=' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::EqEq
                } else if self.peek() == Some('>') {
                    self.bump();
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }
            ':' => {
                if self.peek() == Some(':') {
                    self.bump();
                    TokenKind::DoubleColon
                } else {
                    TokenKind::Colon
                }
            }
            ';' => TokenKind::Semi,
            ',' => TokenKind::Comma,
            '.' => {
                if self.peek() == Some('.') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::DotDotEq
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '@' => TokenKind::At,
            '#' => TokenKind::Hash,
            '?' => TokenKind::Question,

            // Identifiers
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut s = String::new();
                s.push(c);
                s.push_str(&self.read_while(|c| c.is_alphanumeric() || c == '_'));
                return Token::new(Self::ident_or_keyword(&s), loc);
            }

            c => {
                self.diags.push(Diag::new(
                    "E0004",
                    loc,
                    format!("unexpected character: '{}'", c),
                ));
                TokenKind::Error(format!("unexpected char: {}", c))
            }
        };

        Token::new(kind, loc)
    }
}

impl Iterator for Lexer {
    type Item = Token;
    fn next(&mut self) -> Option<Self::Item> {
        let tok = self.next_token();
        if matches!(tok.kind, TokenKind::Eof) {
            None
        } else {
            Some(tok)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lex = Lexer::new("fn main() { let x: u64 = 42; }");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        assert!(toks.len() >= 8);
        assert!(matches!(toks[0].kind, TokenKind::Fn));
        assert!(matches!(toks[1].kind, TokenKind::Ident(_)));
        assert!(matches!(toks[2].kind, TokenKind::LParen));
        assert!(matches!(toks[3].kind, TokenKind::RParen));
        assert!(matches!(toks[4].kind, TokenKind::LBrace));
        assert!(matches!(toks[5].kind, TokenKind::Let));
    }

    #[test]
    fn test_numbers() {
        let mut lex = Lexer::new("42 0xFF 0b1010");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0].kind, TokenKind::Int(42));
        assert_eq!(toks[1].kind, TokenKind::Int(255));
        assert_eq!(toks[2].kind, TokenKind::Int(10));
    }

    #[test]
    fn test_string() {
        let mut lex = Lexer::new("\"hello world\"");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::StrLit("hello world".to_string()));
    }

    #[test]
    fn test_capability() {
        let mut lex = Lexer::new("cap Cap(Console) =>");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        assert!(matches!(toks[0].kind, TokenKind::Cap));
    }

    #[test]
    fn test_operators() {
        let mut lex = Lexer::new("-> => .. ..= ::");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        assert_eq!(toks.len(), 5, "expected 5 tokens from '-> => .. ..= ::'");
        assert!(matches!(toks[0].kind, TokenKind::RArrow));
        assert!(matches!(toks[1].kind, TokenKind::FatArrow));
        assert!(matches!(toks[2].kind, TokenKind::DotDot));
        assert!(matches!(toks[3].kind, TokenKind::DotDotEq));
        assert!(matches!(toks[4].kind, TokenKind::DoubleColon));
    }

    /// Regression: `<<` and `>>` used to be collapsed into a single `Lt`/`Gt`
    /// token, which silently turned `a << b` into `a < b` and made shifts
    /// impossible to express.
    #[test]
    fn test_shift_operators_are_distinct_from_comparisons() {
        let mut lex = Lexer::new("<< >> < > <= >= <<= >>=");
        let toks: Vec<_> = lex.by_ref().collect();
        assert!(lex.diagnostics().is_empty());
        let kinds: Vec<&TokenKind> = toks.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::Shl));
        assert!(matches!(kinds[1], TokenKind::Shr));
        assert!(matches!(kinds[2], TokenKind::Lt));
        assert!(matches!(kinds[3], TokenKind::Gt));
        assert!(matches!(kinds[4], TokenKind::Le));
        assert!(matches!(kinds[5], TokenKind::Ge));
        assert!(matches!(kinds[6], TokenKind::ShlEq));
        assert!(matches!(kinds[7], TokenKind::ShrEq));
    }
}
