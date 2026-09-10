// Phorc — Phorensic Bootstrap Compiler
// Recursive-descent Parser — implements PHORENSIC_LANGUAGE.md §2-§7

use crate::ast::*;
use crate::lex::{Diag, Lexer, Loc, Token, TokenKind};

type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug)]
pub struct ParseError {
    pub diag: Diag,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diags: Vec<Diag>,
}

impl Parser {
    pub fn new(lexer: &mut Lexer) -> Self {
        let mut tokens: Vec<Token> = lexer.by_ref().collect();
        // Filter out comments
        tokens.retain(|t| !matches!(t.kind, TokenKind::Comment));
        Self {
            tokens,
            pos: 0,
            diags: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> &[Diag] {
        &self.diags
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind.clone())
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn loc(&self) -> Loc {
        self.peek().map(|t| t.loc).unwrap_or(Loc::zero())
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> ParseResult<Token> {
        let tok = self.advance().ok_or_else(|| {
            let loc = self.loc();
            ParseError {
                diag: Diag::new("E0101", loc, format!("expected {:?}, got EOF", kind)),
            }
        })?;
        if std::mem::discriminant(&tok.kind) != std::mem::discriminant(&kind) {
            return Err(ParseError {
                diag: Diag::new(
                    "E0102",
                    tok.loc,
                    format!("expected {:?}, got {:?}", kind, tok.kind),
                ),
            });
        }
        Ok(tok)
    }

    #[allow(dead_code)]
    fn expect_any(&mut self, kinds: &[TokenKind]) -> ParseResult<Token> {
        let tok = self.advance().ok_or_else(|| ParseError {
            diag: Diag::new("E0101", self.loc(), "expected token, got EOF"),
        })?;
        if kinds
            .iter()
            .any(|k| std::mem::discriminant(k) == std::mem::discriminant(&tok.kind))
        {
            Ok(tok)
        } else {
            Err(ParseError {
                diag: Diag::new(
                    "E0102",
                    tok.loc,
                    format!("unexpected token: {:?}", tok.kind),
                ),
            })
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek().map_or(false, |t| {
            std::mem::discriminant(&t.kind) == std::mem::discriminant(kind)
        })
    }

    fn consume_if(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_if_any_ident(&mut self) -> bool {
        if self.check_any_ident() {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check_any_ident(&self) -> bool {
        self.peek().map_or(false, |t| {
            matches!(t.kind, TokenKind::Ident(_)) || is_ident_like_keyword(&t.kind)
        })
    }

    fn check_str_lit(&self) -> bool {
        self.peek()
            .map_or(false, |t| matches!(t.kind, TokenKind::StrLit(_)))
    }

    // === Top level ===

    pub fn parse_source(&mut self) -> Result<SourceFile, ()> {
        let loc = self.loc();
        let mut items = Vec::new();
        while self.peek().is_some() {
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.diags.push(e.diag);
                    self.sync_to_item();
                }
            }
        }
        // Even with diags, return the partial AST for continued compilation
        // This allows the pipeline to proceed even with parse errors
        if !self.diags.is_empty() {
            // Still return the items we managed to parse
        }
        Ok(SourceFile { items, loc })
    }

    fn sync_to_item(&mut self) {
        while let Some(tok) = self.peek() {
            match &tok.kind {
                TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Type
                | TokenKind::Package
                | TokenKind::Pub
                | TokenKind::Use
                | TokenKind::Import
                | TokenKind::Impl
                | TokenKind::Const
                | TokenKind::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_item(&mut self) -> ParseResult<Item> {
        let loc = self.loc();
        let vis = if self.consume_if(&TokenKind::Pub) {
            Visibility::Pub
        } else {
            Visibility::Private
        };
        match self.peek_kind() {
            Some(TokenKind::Fn) => {
                self.advance();
                Ok(Item::Fn(self.parse_fn_decl(vis)?))
            }
            Some(TokenKind::Struct) => {
                self.advance();
                Ok(Item::Struct(self.parse_struct_decl(vis)?))
            }
            Some(TokenKind::Enum) => {
                self.advance();
                Ok(Item::Enum(self.parse_enum_decl(vis)?))
            }
            Some(TokenKind::Type) => {
                self.advance();
                Ok(Item::TypeAlias(self.parse_type_alias()?))
            }
            Some(TokenKind::Use) | Some(TokenKind::Import) => {
                self.advance();
                // Consume tokens until next top-level keyword (newline-terminated in .phor)
                while self.peek().is_some() {
                    let next = self.peek_kind();
                    match next {
                        Some(TokenKind::Fn)
                        | Some(TokenKind::Struct)
                        | Some(TokenKind::Enum)
                        | Some(TokenKind::Type)
                        | Some(TokenKind::Pub)
                        | Some(TokenKind::Use)
                        | Some(TokenKind::Import)
                        | Some(TokenKind::Impl)
                        | Some(TokenKind::Const)
                        | Some(TokenKind::Package)
                        | Some(TokenKind::Semi)
                        | Some(TokenKind::Eof) => break,
                        _ => {
                            self.advance();
                        }
                    }
                }
                self.consume_if(&TokenKind::Semi);
                Ok(Item::Import(
                    Ident {
                        name: "_import".to_string(),
                        loc,
                    },
                    loc,
                ))
            }
            Some(TokenKind::Impl) => {
                self.advance();
                self.parse_impl_block(vis)
            }
            Some(TokenKind::Const) => {
                self.advance();
                self.parse_const_decl(vis)
            }
            Some(TokenKind::Package) => {
                self.advance();
                if self.check_str_lit() {
                    // package "name:version";
                    let _ = self.advance();
                    self.consume_if(&TokenKind::Semi);
                    let ident = Ident {
                        name: "package".to_string(),
                        loc,
                    };
                    Ok(Item::Package(ident, loc))
                } else {
                    // package name (possibly dotted like kernel.main)
                    let name = self.parse_ident()?;
                    // Consume any additional path segments (e.g., .main)
                    let mut package_name = name.name.clone();
                    while self.consume_if(&TokenKind::Dot) {
                        if let Ok(seg) = self.parse_ident() {
                            package_name.push('.');
                            package_name.push_str(&seg.name);
                        }
                    }
                    self.consume_if(&TokenKind::Semi);
                    Ok(Item::Package(
                        Ident {
                            name: package_name,
                            loc: name.loc,
                        },
                        loc,
                    ))
                }
            }
            _ => {
                return Err(ParseError {
                    diag: Diag::new(
                        "E0103",
                        loc,
                        format!("unexpected token at top level: {:?}", self.peek_kind()),
                    ),
                });
            }
        }
    }

    fn parse_impl_block(&mut self, vis: Visibility) -> ParseResult<Item> {
        let loc = self.loc();
        let owner = self.parse_ident()?;
        self.expect(TokenKind::LBrace)?;
        let mut methods = Vec::new();
        let mut brace_depth: u32 = 1;
        while brace_depth > 0 && self.peek().is_some() {
            // Consume `pub` or `pub()` if present
            let _ = self.consume_if(&TokenKind::Pub);
            // If we hit `fn`, parse a method with save/restore on failure
            if self.consume_if(&TokenKind::Fn) {
                let save_pos = self.pos;
                match self.parse_fn_decl(Visibility::Private) {
                    Ok(method) => methods.push(method),
                    Err(e) => {
                        self.diags.push(e.diag);
                        // Restore to before `fn` so recovery can skip the failed declaration
                        self.pos = save_pos;
                        // Skip everything that belongs to this failed declaration:
                        // advance past `fn` and scan to next `pub`/`fn`/`}` at impl depth (brace_depth)
                        let _ = self.advance(); // skip `fn`
                        let mut depth: u32 = 0;
                        loop {
                            match self.peek_kind() {
                                Some(TokenKind::LBrace) => {
                                    depth += 1;
                                    self.advance();
                                }
                                Some(TokenKind::RBrace) => {
                                    if depth == 0 {
                                        brace_depth -= 1; // impl closing brace
                                        break;
                                    }
                                    depth -= 1;
                                    self.advance();
                                    if depth == 0 {
                                        // We've exited the failed function's body.
                                        // Check if next tokens are pub/fn for another method
                                        break;
                                    }
                                }
                                Some(TokenKind::Pub) | Some(TokenKind::Fn) if depth == 0 => break,
                                Some(_) => {
                                    self.advance();
                                }
                                None => break,
                            }
                        }
                    }
                }
            } else if self.consume_if(&TokenKind::RBrace) {
                brace_depth -= 1;
            } else if self.consume_if(&TokenKind::LBrace) {
                brace_depth += 1;
            } else {
                self.advance();
            }
        }
        if methods.is_empty() {
            return Err(ParseError {
                diag: Diag::new("E0110", loc, "empty impl block".to_string()),
            });
        }
        Ok(Item::Impl(crate::ast::ImplBlock {
            owner,
            methods,
            visibility: vis,
            loc,
        }))
    }

    fn parse_const_decl(&mut self, vis: Visibility) -> ParseResult<Item> {
        let loc = self.loc();
        let name = self.parse_ident()?;
        let type_ = if self.consume_if(&TokenKind::Colon) {
            if !self.check(&TokenKind::Eq) {
                Some(self.parse_type()?)
            } else {
                None
            }
        } else {
            None
        };
        let value = if self.consume_if(&TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.consume_if(&TokenKind::Semi);
        Ok(Item::Const(ConstDecl {
            name,
            type_,
            value,
            visibility: vis,
            loc,
        }))
    }

    // === Identifiers ===

    fn parse_ident(&mut self) -> ParseResult<Ident> {
        let tok = self.advance().ok_or_else(|| ParseError {
            diag: Diag::new("E0101", self.loc(), "expected identifier"),
        })?;
        // Accept keyword tokens as identifiers when they appear in identifier position
        // This is needed because .phor uses keywords like "trust", "type", "mode" as field/type names
        if let TokenKind::Ident(name) = tok.kind {
            Ok(Ident { name, loc: tok.loc })
        } else if is_ident_like_keyword(&tok.kind) {
            Ok(Ident {
                name: keyword_to_string(&tok.kind),
                loc: tok.loc,
            })
        } else {
            Err(ParseError {
                diag: Diag::new(
                    "E0102",
                    tok.loc,
                    format!("expected identifier, got {:?}", tok.kind),
                ),
            })
        }
    }

    /// Expect a semicolon, but allow it to be missing before a closing brace (newline-terminated .phor)
    #[allow(dead_code)]
    fn expect_semi_opt(&mut self) {
        if !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            self.consume_if(&TokenKind::Semi);
        } else {
            self.consume_if(&TokenKind::Semi);
        }
    }

    #[allow(dead_code)]
    fn parse_ident_raw(&mut self) -> ParseResult<Ident> {
        let tok = self.advance().ok_or_else(|| ParseError {
            diag: Diag::new("E0101", self.loc(), "expected identifier"),
        })?;
        match tok.kind {
            TokenKind::Ident(name) => Ok(Ident { name, loc: tok.loc }),
            other => Err(ParseError {
                diag: Diag::new(
                    "E0102",
                    tok.loc,
                    format!("expected identifier, got {:?}", other),
                ),
            }),
        }
    }

    // === Type parsing ===

    fn parse_type(&mut self) -> ParseResult<TypeExpr> {
        let loc = self.loc();
        let result = self.parse_type_inner()?;
        self.parse_type_suffix(result, loc)
    }

    fn parse_type_inner(&mut self) -> ParseResult<TypeExpr> {
        let loc = self.loc();
        match self.peek_kind() {
            Some(TokenKind::U8) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::U8, loc))
            }
            Some(TokenKind::U16) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::U16, loc))
            }
            Some(TokenKind::U32) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::U32, loc))
            }
            Some(TokenKind::U64) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::U64, loc))
            }
            Some(TokenKind::I8) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::I8, loc))
            }
            Some(TokenKind::I16) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::I16, loc))
            }
            Some(TokenKind::I32) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::I32, loc))
            }
            Some(TokenKind::I64) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::I64, loc))
            }
            Some(TokenKind::BoolTy) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Bool, loc))
            }
            Some(TokenKind::CharTy) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Char, loc))
            }
            Some(TokenKind::F32) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::F32, loc))
            }
            Some(TokenKind::F64) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::F64, loc))
            }
            Some(TokenKind::Usize) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Usize, loc))
            }
            Some(TokenKind::Isize) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Isize, loc))
            }
            Some(TokenKind::Void) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Void, loc))
            }
            Some(TokenKind::Never) => {
                self.advance();
                Ok(TypeExpr::Never(loc))
            }
            Some(TokenKind::Provenance) => {
                self.advance();
                Ok(TypeExpr::Provenance(loc))
            }
            Some(TokenKind::Receipt) => {
                self.advance();
                Ok(TypeExpr::Receipt(loc))
            }
            Some(TokenKind::TrustState) => {
                self.advance();
                Ok(TypeExpr::TrustState(loc))
            }
            Some(TokenKind::ResidualTy) => {
                self.advance();
                Ok(TypeExpr::ResidualTy(loc))
            }
            Some(TokenKind::Effect) => {
                self.advance();
                Ok(TypeExpr::Effect(loc))
            }

            Some(TokenKind::Cap) => {
                self.advance();
                // Expect optional parens: cap(u64) or cap u64
                let has_paren = self.consume_if(&TokenKind::LParen);
                let inner = self.parse_type()?;
                if has_paren {
                    self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Cap(Box::new(inner), loc))
            }
            Some(TokenKind::Handle) => {
                self.advance();
                let has_paren = self.consume_if(&TokenKind::LParen);
                let inner = self.parse_type()?;
                if has_paren {
                    self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Handle(Box::new(inner), loc))
            }
            Some(TokenKind::Slice) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let inner = self.parse_type()?;
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Slice(Box::new(inner), loc))
            }
            Some(TokenKind::Array) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let inner = self.parse_type()?;
                let _ = self.expect(TokenKind::Comma)?;
                let size = match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(n),
                        loc: iloc,
                        ..
                    }) => ArraySize::Literal(n, iloc),
                    Some(Token {
                        kind: TokenKind::Ident(name),
                        loc: iloc,
                        ..
                    }) => ArraySize::ConstIdent(name, iloc),
                    _ => {
                        return Err(ParseError {
                            diag: Diag::new(
                                "E0104",
                                self.loc(),
                                "expected compile-time integer or constant for Array size",
                            ),
                        })
                    }
                };
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Array(Box::new(inner), size, loc))
            }
            Some(TokenKind::RingBuf) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let inner = self.parse_type()?;
                let _ = self.expect(TokenKind::Comma)?;
                let size = match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(n),
                        loc: iloc,
                        ..
                    }) => ArraySize::Literal(n, iloc),
                    Some(Token {
                        kind: TokenKind::Ident(name),
                        loc: iloc,
                        ..
                    }) => ArraySize::ConstIdent(name, iloc),
                    _ => {
                        return Err(ParseError {
                            diag: Diag::new(
                                "E0104",
                                self.loc(),
                                "expected compile-time integer or constant for RingBuf size",
                            ),
                        })
                    }
                };
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::RingBuf(Box::new(inner), size, loc))
            }
            Some(TokenKind::StrTy) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let size = match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(n),
                        loc: iloc,
                        ..
                    }) => ArraySize::Literal(n, iloc),
                    Some(Token {
                        kind: TokenKind::Ident(name),
                        loc: iloc,
                        ..
                    }) => ArraySize::ConstIdent(name, iloc),
                    _ => {
                        return Err(ParseError {
                            diag: Diag::new(
                                "E0104",
                                self.loc(),
                                "expected compile-time integer or constant for Str size",
                            ),
                        })
                    }
                };
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Str(size, loc))
            }
            Some(TokenKind::Result) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let ok_ty = self.parse_type()?;
                let _ = self.expect(TokenKind::Comma)?;
                let err_ty = self.parse_type()?;
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Result(Box::new(ok_ty), Box::new(err_ty), loc))
            }
            Some(TokenKind::Option) => {
                self.advance();
                let use_bracket = self.consume_if(&TokenKind::LBracket);
                if !use_bracket {
                    let _ = self.expect(TokenKind::LParen)?;
                }
                let inner = self.parse_type()?;
                if use_bracket {
                    self.expect(TokenKind::RBracket)?;
                } else {
                    let _ = self.expect(TokenKind::RParen)?;
                }
                Ok(TypeExpr::Option(Box::new(inner), loc))
            }
            Some(TokenKind::LParen) => {
                self.advance();
                // Support empty tuple () as void, and single-element tuple as plain type
                if self.check(&TokenKind::RParen) {
                    self.advance();
                    return Ok(TypeExpr::Prim(PrimType::Void, loc));
                }
                let first = self.parse_type()?;
                if self.consume_if(&TokenKind::Comma) {
                    let mut types = vec![first];
                    if !self.check(&TokenKind::RParen) {
                        types.push(self.parse_type()?);
                        while self.consume_if(&TokenKind::Comma) {
                            if self.check(&TokenKind::RParen) {
                                break;
                            }
                            types.push(self.parse_type()?);
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(TypeExpr::Tuple(types, loc))
                } else {
                    self.expect(TokenKind::RParen)?;
                    // Single element in parens is not a tuple — unwrap
                    Ok(first)
                }
            }

            Some(TokenKind::Ref) | Some(TokenKind::Amp) => {
                // ref T, ref mut T, &T, self: ref mut — reference modifier
                self.advance();
                self.consume_if(&TokenKind::Mut);
                // If next token is a type start, parse it; otherwise return void placeholder
                if self.peek().map_or(false, |t| {
                    !matches!(
                        t.kind,
                        TokenKind::Comma
                            | TokenKind::RParen
                            | TokenKind::RBrace
                            | TokenKind::RBracket
                            | TokenKind::Eq
                            | TokenKind::Semi
                    )
                }) {
                    self.parse_type()
                } else {
                    Ok(TypeExpr::Prim(PrimType::Void, loc))
                }
            }

            Some(TokenKind::Ident(_)) => {
                let ident = self.parse_ident()?;
                Ok(TypeExpr::Named(ident))
            }

            // Integer literals can appear as type-size parameters
            Some(TokenKind::Int(_n)) => {
                self.advance();
                Ok(TypeExpr::Prim(PrimType::Usize, loc))
            }

            other => Err(ParseError {
                diag: Diag::new("E0105", loc, format!("expected type, got {:?}", other)),
            }),
        }
    }

    /// Parse a suffix of Ident[N], Ident(N), Ident[T,N] if present (parameterized named types)
    /// Handles both bracket and paren generics: Result[T, E], Option[T], Array[T, N]
    fn parse_type_suffix(&mut self, ty: TypeExpr, loc: Loc) -> ParseResult<TypeExpr> {
        // If we have a named type followed by bracket/paren, try to parse as generic type
        if matches!(&ty, TypeExpr::Named(ident) if ident.name == "Result" || ident.name == "Option"
            || ident.name == "Array" || ident.name == "FixedArray")
        {
            if self.check(&TokenKind::LBracket) {
                // Parse Result[T, E] or Option[T] or Array[T, N]
                self.advance(); // consume [
                let inner = self.parse_type()?;
                let second = if self.consume_if(&TokenKind::Comma) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(TokenKind::RBracket)?;
                match ty {
                    TypeExpr::Named(ref ident) if ident.name == "Result" => Ok(TypeExpr::Result(
                        Box::new(inner),
                        Box::new(second.unwrap_or_else(|| TypeExpr::Prim(PrimType::Void, loc))),
                        loc,
                    )),
                    TypeExpr::Named(ref ident) if ident.name == "Option" => {
                        Ok(TypeExpr::Option(Box::new(inner), loc))
                    }
                    TypeExpr::Named(ref ident)
                        if ident.name == "Array" || ident.name == "FixedArray" =>
                    {
                        // Extract size from the second param if possible
                        let array_size = match &second {
                            Some(TypeExpr::Prim(PrimType::Usize, iloc)) => {
                                ArraySize::Literal(0, *iloc)
                            }
                            _ => ArraySize::Literal(0, loc),
                        };
                        Ok(TypeExpr::Array(Box::new(inner), array_size, loc))
                    }
                    _ => Ok(ty),
                }
            } else if self.check(&TokenKind::LParen) {
                // Parenthesized syntax: Result(T, E)
                let _ = self.advance();
                let mut depth = 1;
                while depth > 0 && self.peek().is_some() {
                    if self.consume_if(&TokenKind::LParen) {
                        depth += 1;
                    } else if self.consume_if(&TokenKind::RParen) {
                        depth -= 1;
                    } else {
                        self.advance();
                    }
                }
                Ok(ty)
            } else {
                Ok(ty)
            }
        } else if self.check(&TokenKind::LBracket) || self.check(&TokenKind::LParen) {
            // Other named types with bracket/paren suffix — skip content
            let _open = if self.consume_if(&TokenKind::LBracket) {
                TokenKind::LBracket
            } else {
                self.advance(); // consume (
                TokenKind::LParen
            };
            let mut depth = 1;
            while depth > 0 && self.peek().is_some() {
                if self.consume_if(&TokenKind::LBracket) || self.consume_if(&TokenKind::LParen) {
                    depth += 1;
                } else if self.consume_if(&TokenKind::RBracket)
                    || self.consume_if(&TokenKind::RParen)
                {
                    depth -= 1;
                } else {
                    self.advance();
                }
            }
            Ok(ty)
        } else {
            Ok(ty)
        }
    }

    // === Function declarations ===

    fn parse_fn_decl(&mut self, vis: Visibility) -> ParseResult<FnDecl> {
        let loc = self.loc();
        let name = self.parse_ident()?;

        // Parameters
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                // Skip ref/mut prefixes on parameters
                self.consume_if(&TokenKind::Ref);
                self.consume_if(&TokenKind::Mut);
                // Break if next is closing paren (e.g., trailing comma handled)
                if self.check(&TokenKind::RParen) {
                    break;
                }
                let p_name = self.parse_ident()?;
                // Type annotation may be omitted (e.g., `self` has implicit type)
                if self.consume_if(&TokenKind::Colon) {
                    let p_type = self.parse_type()?;
                    params.push((p_name, p_type));
                } else {
                    // No type annotation — use a placeholder
                    params.push((p_name, TypeExpr::Prim(PrimType::Void, loc)));
                }
                if !self.consume_if(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;

        // Return type (optional)
        let return_type = if self.consume_if(&TokenKind::RArrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Effects
        let mut effects = Vec::new();
        if self.consume_if(&TokenKind::Effect) {
            self.expect(TokenKind::LBracket)?;
            loop {
                if self.check(&TokenKind::RBracket) {
                    break;
                }
                let eff = self.parse_effect()?;
                effects.push(eff);
                if !self.consume_if(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBracket)?;
        }

        // Also handle "effects:" syntax (with colon and ident)
        if self.peek().map_or(
            false,
            |t| matches!(&t.kind, TokenKind::Ident(s) if s == "effects"),
        ) {
            if self
                .peek_at(1)
                .map_or(false, |t| matches!(&t.kind, TokenKind::Colon))
            {
                self.advance(); // consume 'effects'
                self.advance(); // consume ':'
                                // Now parse effect[...] same as above
                self.expect(TokenKind::LBracket)?;
                loop {
                    if self.check(&TokenKind::RBracket) {
                        break;
                    }
                    let eff = self.parse_effect()?;
                    effects.push(eff);
                    if !self.consume_if(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBracket)?;
            }
        }

        // Court annotation
        let court = if self.consume_if(&TokenKind::Court) {
            match self.advance() {
                Some(Token {
                    kind: TokenKind::StrLit(s),
                    ..
                }) => Some(s),
                _ => {
                    return Err(ParseError {
                        diag: Diag::new("E0106", self.loc(), "expected court string"),
                    })
                }
            }
        } else {
            None
        };

        // Bound annotation
        let bound = if self.consume_if(&TokenKind::Bound) {
            match self.advance() {
                Some(Token {
                    kind: TokenKind::Ident(what),
                    ..
                }) => match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(v),
                        ..
                    }) => Some((what, v)),
                    _ => {
                        return Err(ParseError {
                            diag: Diag::new("E0107", self.loc(), "expected bound value"),
                        })
                    }
                },
                _ => {
                    return Err(ParseError {
                        diag: Diag::new("E0107", self.loc(), "expected bound annotation"),
                    })
                }
            }
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(FnDecl {
            name,
            params,
            effects,
            return_type,
            court,
            bound,
            body,
            visibility: vis,
            loc,
        })
    }

    fn parse_effect(&mut self) -> ParseResult<Effect> {
        let mut name = self.parse_ident()?.name;
        // Handle effect names with colons: io:write, machine:ioport, memory:mmio, etc.
        while self.consume_if(&TokenKind::Colon) {
            name.push(':');
            name.push_str(&self.parse_ident()?.name);
        }
        Ok(match name.as_str() {
            "io:read" => Effect::IORead,
            "io:write" => Effect::IOWrite,
            "compute" => Effect::Compute,
            "blocking" => Effect::Blocking,
            "irq:handle" => Effect::IrqHandle,
            "residual" => Effect::Residual,
            "machine:ioport" => Effect::MachineIoport,
            "memory:mmio" => Effect::MemoryMMIO,
            "cage:translate" => Effect::CageTranslate,
            "court:request" => Effect::CourtRequest,
            "dma" => Effect::Dma,
            other => Effect::Named(other.to_string()),
        })
    }

    // === Block parsing ===

    fn parse_block(&mut self) -> ParseResult<Block> {
        let loc = self.loc();
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Block { stmts, loc })
    }

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        let loc = self.loc();
        match self.peek_kind() {
            Some(TokenKind::Let) => {
                self.advance();
                let mut_ = self.consume_if(&TokenKind::Mut);
                let name = self.parse_ident()?;
                let type_ann = if self.consume_if(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let init = if self.consume_if(&TokenKind::Eq) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                // Semicolons are always optional in .phor (newline-terminated)
                self.consume_if(&TokenKind::Semi);
                Ok(Stmt::Let {
                    name,
                    type_ann,
                    init,
                    mut_,
                    loc,
                })
            }
            Some(TokenKind::Return) => {
                self.advance();
                let expr = if !self.check(&TokenKind::Semi) && !self.check(&TokenKind::RBrace) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                // Semicolons are always optional in .phor
                self.consume_if(&TokenKind::Semi);
                Ok(Stmt::Return(expr, loc))
            }
            Some(TokenKind::Const) => {
                // Inline const declaration inside function body.
                // Parse and discard the tokens so recovery continues to the next statement.
                self.advance(); // consume 'const'
                                // Parse the constant name
                if self.check_any_ident() {
                    self.advance();
                    // Skip : Type = Value
                    self.consume_if(&TokenKind::Colon);
                    // Skip type tokens until =
                    while self.peek().is_some()
                        && !self.check(&TokenKind::Eq)
                        && !self.check(&TokenKind::Semi)
                        && !self.check(&TokenKind::RBrace)
                    {
                        self.advance();
                    }
                    self.consume_if(&TokenKind::Eq);
                    // Skip the value expression until ; or }
                    while self.peek().is_some()
                        && !self.check(&TokenKind::Semi)
                        && !self.check(&TokenKind::RBrace)
                    {
                        self.advance();
                    }
                }
                self.consume_if(&TokenKind::Semi);
                Ok(Stmt::Expr(Expr::Literal(Literal::Int(0, loc)), loc))
            }
            Some(TokenKind::Enum) | Some(TokenKind::Struct) | Some(TokenKind::Type) => {
                // Local type declaration inside function body.
                // Skip the entire declaration to avoid cascading errors.
                self.advance(); // consume enum/struct/type
                                // Skip name and brace body
                if self.check_any_ident() {
                    self.advance();
                    if self.consume_if(&TokenKind::LBrace) {
                        let mut depth = 1;
                        while depth > 0 && self.peek().is_some() {
                            if self.consume_if(&TokenKind::LBrace) {
                                depth += 1;
                            } else if self.consume_if(&TokenKind::RBrace) {
                                depth -= 1;
                            } else {
                                self.advance();
                            }
                        }
                    } else {
                        // No brace — skip until semicolon or brace
                        while self.peek().is_some()
                            && !self.check(&TokenKind::Semi)
                            && !self.check(&TokenKind::RBrace)
                        {
                            self.advance();
                        }
                    }
                }
                self.consume_if(&TokenKind::Semi);
                Ok(Stmt::Expr(Expr::Literal(Literal::Int(0, loc)), loc))
            }
            Some(TokenKind::Residual) => {
                self.advance();
                // Expect "emit" keyword
                if self.check_any_ident() {
                    if self.peek().map_or(
                        false,
                        |t| matches!(&t.kind, TokenKind::Ident(s) if s == "emit"),
                    ) {
                        self.advance();
                    }
                }
                // Parse { op: "opname", field: value, ... }
                self.expect(TokenKind::LBrace)?;
                let mut op = String::new();
                let mut fields: Vec<(String, Expr)> = Vec::new();
                while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
                    let field_name = self.parse_ident()?;
                    self.expect(TokenKind::Colon)?;
                    let field_val = self.parse_expr()?;
                    if field_name.name == "op" {
                        // Extract the operation name from the string literal
                        if let Expr::Literal(Literal::Str(s, _)) = &field_val {
                            op = s.clone();
                        } else {
                            // Fallback: use the expression string representation
                            op = field_name.name.clone();
                        }
                    } else {
                        fields.push((field_name.name, field_val));
                    }
                    if !self.consume_if(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                self.consume_if(&TokenKind::Semi);
                Ok(Stmt::Expr(Expr::ResidualEmit { op, fields, loc }, loc))
            }
            _ => {
                let expr = self.parse_expr()?;
                if self.consume_if(&TokenKind::Eq) {
                    let value = self.parse_expr()?;
                    self.consume_if(&TokenKind::Semi);
                    Ok(Stmt::Assignment {
                        target: Box::new(expr),
                        value: Box::new(value),
                        loc,
                    })
                } else {
                    // Semicolons are always optional in .phor
                    self.consume_if(&TokenKind::Semi);
                    Ok(Stmt::Expr(expr, loc))
                }
            }
        }
    }

    // === Expression parsing (Precedence climbing) ===

    fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_assignment_expr()
    }

    fn parse_assignment_expr(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_or_expr()?;
        if self.check(&TokenKind::Eq) {
            self.advance();
            let rhs = self.parse_assignment_expr()?;
            Ok(Expr::Binary {
                op: BinOp::Assign,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            })
        } else {
            Ok(lhs)
        }
    }

    fn parse_or_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_bit_or_expr()?;
        while self.consume_if(&TokenKind::OrOr) {
            let rhs = self.parse_bit_or_expr()?;
            lhs = Expr::Binary {
                op: BinOp::OrOr,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            };
        }
        Ok(lhs)
    }

    fn parse_bit_or_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_bit_xor_expr()?;
        while self.consume_if(&TokenKind::Pipe) {
            let rhs = self.parse_bit_xor_expr()?;
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            };
        }
        Ok(lhs)
    }

    fn parse_bit_xor_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_bit_and_expr()?;
        while self.consume_if(&TokenKind::Caret) {
            let rhs = self.parse_bit_and_expr()?;
            lhs = Expr::Binary {
                op: BinOp::Xor,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            };
        }
        Ok(lhs)
    }

    fn parse_bit_and_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_and_expr()?;
        while self.consume_if(&TokenKind::Amp) {
            let rhs = self.parse_and_expr()?;
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            };
        }
        Ok(lhs)
    }

    fn parse_and_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_comparison_expr()?;
        while self.consume_if(&TokenKind::AndAnd) {
            let rhs = self.parse_comparison_expr()?;
            lhs = Expr::Binary {
                op: BinOp::AndAnd,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc: self.loc(),
            };
        }
        Ok(lhs)
    }

    fn parse_comparison_expr(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_additive_expr()?;
        let loc = self.loc();
        if let Some(op) = match self.peek_kind() {
            Some(TokenKind::EqEq) => Some(BinOp::Eq),
            Some(TokenKind::Neq) => Some(BinOp::Ne),
            Some(TokenKind::Lt) => Some(BinOp::Lt),
            Some(TokenKind::Le) => Some(BinOp::Le),
            Some(TokenKind::Gt) => Some(BinOp::Gt),
            Some(TokenKind::Ge) => Some(BinOp::Ge),
            _ => None,
        } {
            self.advance();
            let rhs = self.parse_additive_expr()?;
            Ok(Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc,
            })
        } else {
            Ok(lhs)
        }
    }

    fn parse_additive_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_range_expr()?;
        loop {
            let loc = self.loc();
            match self.peek_kind() {
                Some(TokenKind::Plus) => {
                    self.advance();
                    lhs = Expr::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(lhs),
                        rhs: Box::new(self.parse_range_expr()?),
                        loc,
                    };
                }
                Some(TokenKind::Minus) => {
                    self.advance();
                    lhs = Expr::Binary {
                        op: BinOp::Sub,
                        lhs: Box::new(lhs),
                        rhs: Box::new(self.parse_range_expr()?),
                        loc,
                    };
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_range_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_multiplicative_expr()?;
        let loc = self.loc();
        if self.consume_if(&TokenKind::DotDot) {
            let rhs = self.parse_multiplicative_expr()?;
            lhs = Expr::Binary {
                op: BinOp::Range,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc,
            };
        } else if self.consume_if(&TokenKind::DotDotEq) {
            let rhs = self.parse_multiplicative_expr()?;
            lhs = Expr::Binary {
                op: BinOp::RangeInclusive,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                loc,
            };
        }
        Ok(lhs)
    }

    fn parse_multiplicative_expr(&mut self) -> ParseResult<Expr> {
        let mut lhs = self.parse_unary_expr()?;
        loop {
            let loc = self.loc();
            match self.peek_kind() {
                Some(TokenKind::Star) => {
                    self.advance();
                    lhs = Expr::Binary {
                        op: BinOp::Mul,
                        lhs: Box::new(lhs),
                        rhs: Box::new(self.parse_unary_expr()?),
                        loc,
                    };
                }
                Some(TokenKind::Slash) => {
                    self.advance();
                    lhs = Expr::Binary {
                        op: BinOp::Div,
                        lhs: Box::new(lhs),
                        rhs: Box::new(self.parse_unary_expr()?),
                        loc,
                    };
                }
                Some(TokenKind::Percent) => {
                    self.advance();
                    lhs = Expr::Binary {
                        op: BinOp::Rem,
                        lhs: Box::new(lhs),
                        rhs: Box::new(self.parse_unary_expr()?),
                        loc,
                    };
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_unary_expr(&mut self) -> ParseResult<Expr> {
        let loc = self.loc();
        match self.peek_kind() {
            Some(TokenKind::Minus) => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(self.parse_unary_expr()?),
                    loc,
                })
            }
            Some(TokenKind::Exclaim) => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(self.parse_unary_expr()?),
                    loc,
                })
            }
            _ => self.parse_primary_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> ParseResult<Expr> {
        let loc = self.loc();
        // Clone the peek to avoid borrow conflicts with self.advance()
        let peeked = self.peek().cloned();
        let expr = match peeked.as_ref().map(|t| &t.kind) {
            Some(TokenKind::Int(n)) => {
                self.advance();
                Expr::Literal(Literal::Int(*n, loc))
            }
            Some(TokenKind::Float(n)) => {
                self.advance();
                Expr::Literal(Literal::Float(*n, loc))
            }
            Some(TokenKind::True) => {
                self.advance();
                Expr::Literal(Literal::Bool(true, loc))
            }
            Some(TokenKind::False) => {
                self.advance();
                Expr::Literal(Literal::Bool(false, loc))
            }
            Some(TokenKind::Char(c)) => {
                self.advance();
                Expr::Literal(Literal::Char(*c, loc))
            }
            Some(TokenKind::StrLit(s)) => {
                self.advance();
                Expr::Literal(Literal::Str(s.clone(), loc))
            }
            Some(TokenKind::Ident(_)) => {
                let name = self.parse_ident()?;
                Expr::Ident(name)
            }
            // Type keywords used as identifiers in expression context (Handle, Slice, Cap, etc.)
            Some(TokenKind::Handle) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Handle".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Slice) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Slice".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Cap) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Cap".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Array) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Array".to_string(),
                    loc,
                })
            }
            Some(TokenKind::RingBuf) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "RingBuf".to_string(),
                    loc,
                })
            }
            Some(TokenKind::StrTy) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Str".to_string(),
                    loc,
                })
            }
            Some(TokenKind::TrustState) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "TrustState".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Provenance) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Provenance".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Receipt) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Receipt".to_string(),
                    loc,
                })
            }
            Some(TokenKind::ResidualTy) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Residual".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Result) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Result".to_string(),
                    loc,
                })
            }
            Some(TokenKind::Option) => {
                self.advance();
                Expr::TypeIdent(Ident {
                    name: "Option".to_string(),
                    loc,
                })
            }
            Some(TokenKind::LParen) => {
                self.advance();
                let inner = self.parse_expr()?;
                // If followed by comma, this is a tuple literal — parse and discard remaining
                if self.consume_if(&TokenKind::Comma) {
                    while !self.check(&TokenKind::RParen) && self.peek().is_some() {
                        self.parse_expr()?;
                        if !self.consume_if(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RParen)?;
                inner
            }
            Some(TokenKind::LBracket) => {
                self.advance();
                let mut elements = Vec::new();
                if !self.check(&TokenKind::RBracket) {
                    elements.push(self.parse_expr()?);
                    while self.consume_if(&TokenKind::Comma) {
                        if self.check(&TokenKind::RBracket) {
                            break;
                        }
                        elements.push(self.parse_expr()?);
                    }
                }
                self.expect(TokenKind::RBracket)?;
                Expr::ArrayLit(elements, loc)
            }
            Some(TokenKind::LBrace) => {
                let block = self.parse_block()?;
                Expr::Block(block, loc)
            }
            Some(TokenKind::If) => {
                self.advance();
                let cond = self.parse_expr()?;
                let then = self.parse_block()?;
                let else_ = if self.consume_if(&TokenKind::Else) {
                    Some(Box::new(if self.check(&TokenKind::If) {
                        self.parse_expr()?
                    } else {
                        Expr::Block(self.parse_block()?, self.loc())
                    }))
                } else {
                    None
                };
                Expr::If {
                    cond: Box::new(cond),
                    then: Box::new(then),
                    else_,
                    loc,
                }
            }
            Some(TokenKind::Match) => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let mut arms = Vec::new();
                while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
                    let pat = self.parse_pattern()?;
                    let guard = if self.consume_if(&TokenKind::If) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    self.expect(TokenKind::FatArrow)?;
                    let body = self.parse_expr()?;
                    let _ = self.consume_if(&TokenKind::Comma);
                    arms.push(MatchArm {
                        pattern: pat,
                        guard,
                        body: Box::new(body),
                        loc: self.loc(),
                    });
                }
                self.expect(TokenKind::RBrace)?;
                Expr::Match {
                    expr: Box::new(expr),
                    arms,
                    loc,
                }
            }
            Some(TokenKind::Loop) => {
                self.advance();
                let mut bound = None;
                let mut proven = false;
                if self.consume_if(&TokenKind::Bound) {
                    match self.advance() {
                        Some(Token {
                            kind: TokenKind::Int(n),
                            ..
                        }) => bound = Some(n),
                        _ => {
                            return Err(ParseError {
                                diag: Diag::new("E0107", self.loc(), "expected loop bound"),
                            })
                        }
                    }
                }
                if self.consume_if_any_ident()
                    && self.peek().map_or(
                        false,
                        |t| matches!(&t.kind, TokenKind::Ident(name) if name == "proven"),
                    )
                {
                    proven = true;
                    self.advance();
                }
                let body = self.parse_block()?;
                Expr::Loop {
                    body: Box::new(body),
                    bound,
                    proven,
                    loc,
                }
            }
            Some(TokenKind::For) => {
                self.advance();
                let var = self.parse_ident()?;
                self.expect(TokenKind::In)?;
                let range = self.parse_expr()?;
                let body = self.parse_block()?;
                Expr::ForLoop {
                    var,
                    range: Box::new(range),
                    body: Box::new(body),
                    loc,
                }
            }
            Some(TokenKind::While) => {
                self.advance();
                let cond = self.parse_expr()?;
                // Allow optional "bound proven" annotation
                let proven = if self.check_any_ident() {
                    if let Some(Token {
                        kind: TokenKind::Ident(ref s),
                        ..
                    }) = self.peek()
                    {
                        if s == "proven" {
                            self.advance();
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                let body = self.parse_block()?;
                Expr::WhileLoop {
                    cond: Box::new(cond),
                    body: Box::new(body),
                    proven,
                    loc,
                }
            }
            Some(TokenKind::Return) => {
                self.advance();
                let expr = if !self.check(&TokenKind::Semi) && !self.check(&TokenKind::RBrace) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Expr::Return(expr, loc)
            }
            Some(TokenKind::Break) => {
                self.advance();
                Expr::Break(loc)
            }
            Some(TokenKind::Continue) => {
                self.advance();
                Expr::Continue(loc)
            }
            Some(TokenKind::Trusted) | Some(TokenKind::Trust) => {
                self.advance();
                // Parse reason: either `trusted "string" { body }` or `trusted reason "string" { body }`
                let reason = if self.check_str_lit() {
                    // Immediate string: trusted "reason" { }
                    match self.advance() {
                        Some(Token {
                            kind: TokenKind::StrLit(s),
                            ..
                        }) => s,
                        _ => unreachable!(),
                    }
                } else if self.consume_if_any_ident() && self.check_str_lit() {
                    // trusted reason "reason" { }
                    match self.advance() {
                        Some(Token {
                            kind: TokenKind::StrLit(s),
                            ..
                        }) => s,
                        _ => unreachable!(),
                    }
                } else {
                    String::new()
                };
                let body = self.parse_block()?;
                Expr::Trusted {
                    reason,
                    body: Box::new(body),
                    loc,
                }
            }
            other => {
                return Err(ParseError {
                    diag: Diag::new(
                        "E0108",
                        loc,
                        format!("expected expression, got {:?}", other),
                    ),
                });
            }
        };

        // Postfix operations: call, field access, index, method call
        self.parse_postfix(expr)
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> ParseResult<Expr> {
        loop {
            let loc = self.loc();
            // Check for struct literal: Ident { ... } or TypeIdent { ... }
            // Struct literal parsing: Name { field: value, ... }
            // Only trigger when followed by `Name:` pattern (struct field syntax).
            // This avoids consuming `x { stmt; }` after a range expression like `0..x { body }`.
            // Use limited lookahead: check if '{' is followed by Ident ':' within a few tokens.
            let mut likely_struct = false;
            if self.check(&TokenKind::LBrace) {
                let mut lookahead = self.pos + 1;
                // Scan up to 10 tokens ahead
                for _ in 0..10 {
                    match self.tokens.get(lookahead).map(|t| &t.kind) {
                        Some(TokenKind::Colon) => {
                            likely_struct = true;
                            break;
                        }
                        Some(TokenKind::Ident(_)) => {
                            lookahead += 1;
                        }
                        Some(TokenKind::RBrace) | None | Some(TokenKind::Semi) => break,
                        _ => break,
                    }
                }
            }
            let is_type_name = match &expr {
                Expr::Ident(id) => id.name.chars().next().map_or(false, |c| c.is_uppercase()),
                Expr::TypeIdent(_) => true,
                _ => false,
            };
            if is_type_name && likely_struct {
                // Struct literal: Name { field: value, ... }
                self.advance(); // consume {
                let type_name = match &expr {
                    Expr::Ident(id) => id.clone(),
                    Expr::TypeIdent(id) => id.clone(),
                    _ => unreachable!(),
                };
                let mut fields = Vec::new();
                while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
                    let field_name = self.parse_ident()?;
                    self.expect(TokenKind::Colon)?;
                    let field_val = self.parse_expr()?;
                    let _ = self.consume_if(&TokenKind::Comma);
                    fields.push((field_name, field_val));
                }
                self.expect(TokenKind::RBrace)?;
                return Ok(Expr::StructLit {
                    type_name,
                    fields,
                    loc,
                });
            }
            match self.peek_kind() {
                Some(TokenKind::LParen) => {
                    // Check if this is a type constructor like Handle(Type)::method()
                    // Use limited lookahead: if we see `(` and there's a `::` after the matching `)`,
                    // consume the type arguments as raw tokens and let subsequent postfix handle `::method`.
                    let is_qualified_call = {
                        let mut found_paren = false;
                        let mut result = false;
                        if let Some(TokenKind::LParen) = self.peek_kind() {
                            let mut save = self.pos + 1;
                            let mut depth = 1;
                            found_paren = true;
                            // Scan forward to find matching ) and check if :: follows
                            while depth > 0 {
                                match self.tokens.get(save).map(|t| &t.kind) {
                                    Some(TokenKind::LParen) => depth += 1,
                                    Some(TokenKind::RParen) => {
                                        depth -= 1;
                                        // If depth reached 0, check next token for ::
                                        if depth == 0 {
                                            if let Some(TokenKind::DoubleColon) =
                                                self.tokens.get(save + 1).map(|t| &t.kind)
                                            {
                                                result = true;
                                            }
                                        }
                                    }
                                    None => break,
                                    _ => {}
                                }
                                save += 1;
                            }
                        }
                        found_paren && result
                    };
                    if is_qualified_call {
                        // Consume type arguments as raw tokens
                        let _ = self.advance(); // consume (
                        let mut depth = 1;
                        while depth > 0 && self.peek().is_some() {
                            if self.consume_if(&TokenKind::LParen) {
                                depth += 1;
                            } else if self.consume_if(&TokenKind::RParen) {
                                depth -= 1;
                            } else {
                                self.advance();
                            }
                        }
                        // Continue loop — next postfix op (::method) is handled below
                        continue;
                    }
                    // Normal function call
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::RParen) {
                        args.push(self.parse_expr()?);
                        while self.consume_if(&TokenKind::Comma) {
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        loc,
                    };
                }
                Some(TokenKind::Exclaim) => {
                    // Postfix ! operator: expr!
                    self.advance();
                    if self.check(&TokenKind::LParen) {
                        // expr!(args...) — macro-like invocation
                        // Parse as a call with postfix bang
                        self.advance();
                        let mut args = Vec::new();
                        if !self.check(&TokenKind::RParen) {
                            args.push(self.parse_expr()?);
                            while self.consume_if(&TokenKind::Comma) {
                                args.push(self.parse_expr()?);
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                        expr = Expr::Call {
                            callee: Box::new(expr),
                            args,
                            loc,
                        };
                    } else {
                        expr = Expr::PostfixBang {
                            expr: Box::new(expr),
                            loc,
                        };
                    }
                }
                Some(TokenKind::Dot) => {
                    self.advance();
                    // Accept both identifiers and integer literals after '.'
                    // (tuple field access like .0, .1 uses integer literals)
                    let field = if self.check_any_ident() {
                        self.parse_ident()?
                    } else if let Some(Token {
                        kind: TokenKind::Int(n),
                        loc,
                    }) = self.peek().cloned()
                    {
                        self.advance();
                        Ident {
                            name: n.to_string(),
                            loc,
                        }
                    } else {
                        return Err(ParseError {
                            diag: Diag::new(
                                "E0102",
                                self.loc(),
                                format!(
                                    "expected identifier or integer after `.`, got {:?}",
                                    self.peek_kind()
                                ),
                            ),
                        });
                    };
                    if self.check(&TokenKind::LParen) {
                        // Method call
                        self.advance();
                        let mut args = Vec::new();
                        if !self.check(&TokenKind::RParen) {
                            args.push(self.parse_expr()?);
                            while self.consume_if(&TokenKind::Comma) {
                                args.push(self.parse_expr()?);
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                        expr = Expr::MethodCall {
                            obj: Box::new(expr),
                            method: field,
                            args,
                            loc,
                            resolved_owner: None,
                        };
                    } else {
                        expr = Expr::FieldAccess {
                            obj: Box::new(expr),
                            field,
                            loc,
                        };
                    }
                }
                Some(TokenKind::LBracket) => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index {
                        obj: Box::new(expr),
                        index: Box::new(index),
                        loc,
                    };
                }
                Some(TokenKind::DoubleColon) => {
                    self.advance();
                    let field = self.parse_ident()?;
                    if self.check(&TokenKind::LParen) {
                        // Qualified call like Type::method(...)
                        self.advance();
                        let mut args = Vec::new();
                        if !self.check(&TokenKind::RParen) {
                            args.push(self.parse_expr()?);
                            while self.consume_if(&TokenKind::Comma) {
                                args.push(self.parse_expr()?);
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                        let owner_name = match &expr {
                            Expr::TypeIdent(id) => Some(id.name.clone()),
                            Expr::Ident(id)
                                if id.name.chars().next().map_or(false, |c| c.is_uppercase()) =>
                            {
                                Some(id.name.clone())
                            }
                            _ => None,
                        };
                        expr = Expr::MethodCall {
                            obj: Box::new(expr),
                            method: field,
                            args,
                            loc,
                            resolved_owner: owner_name,
                        };
                    } else {
                        // Namespace access like Type::CONSTANT
                        expr = Expr::FieldAccess {
                            obj: Box::new(expr),
                            field,
                            loc,
                        };
                    }
                }
                Some(TokenKind::As) => {
                    self.advance();
                    let target_type = self.parse_type()?;
                    expr = Expr::Cast {
                        expr: Box::new(expr),
                        type_: Box::new(target_type),
                        loc,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    // === Pattern parsing ===

    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let loc = self.loc();
        match self.peek_kind() {
            Some(TokenKind::Int(n)) => {
                self.advance();
                Ok(Pattern::Lit(Literal::Int(n, loc)))
            }
            Some(TokenKind::True) => {
                self.advance();
                Ok(Pattern::Lit(Literal::Bool(true, loc)))
            }
            Some(TokenKind::False) => {
                self.advance();
                Ok(Pattern::Lit(Literal::Bool(false, loc)))
            }
            Some(TokenKind::Result) | Some(TokenKind::Option) | Some(TokenKind::Ident(_)) => {
                // Check for `::` after the identifier/type without consuming it first
                // Look at the next token after the current one
                let has_double_colon = self
                    .peek_at(1)
                    .map_or(false, |t| matches!(&t.kind, TokenKind::DoubleColon));
                if has_double_colon {
                    // Enum variant pattern: Type::Variant(p1, p2, ...)
                    // Consume the type name and capture it as the owner
                    let owner = match self.peek_kind() {
                        Some(TokenKind::Result) => {
                            self.advance();
                            Some(Ident {
                                name: "Result".to_string(),
                                loc,
                            })
                        }
                        Some(TokenKind::Option) => {
                            self.advance();
                            Some(Ident {
                                name: "Option".to_string(),
                                loc,
                            })
                        }
                        Some(TokenKind::Ident(_)) => Some(self.parse_ident()?),
                        _ => None,
                    };
                    self.advance(); // consume ::
                    let variant = self.parse_ident()?;
                    let mut subpatterns = Vec::new();
                    if self.consume_if(&TokenKind::LParen) {
                        if !self.check(&TokenKind::RParen) {
                            subpatterns.push(self.parse_pattern()?);
                            while self.consume_if(&TokenKind::Comma) {
                                if self.check(&TokenKind::RParen) {
                                    break;
                                }
                                subpatterns.push(self.parse_pattern()?);
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                    Ok(Pattern::EnumVariant(owner, variant, subpatterns, loc))
                } else {
                    // Regular identifier pattern
                    match self.peek_kind() {
                        Some(TokenKind::Result) => {
                            self.advance();
                            Ok(Pattern::Ident(Ident {
                                name: "Result".to_string(),
                                loc,
                            }))
                        }
                        Some(TokenKind::Option) => {
                            self.advance();
                            Ok(Pattern::Ident(Ident {
                                name: "Option".to_string(),
                                loc,
                            }))
                        }
                        Some(TokenKind::Ident(_)) => {
                            let ident = self.parse_ident()?;
                            Ok(Pattern::Ident(ident))
                        }
                        _ => Err(ParseError {
                            diag: Diag::new("E0109", loc, "expected pattern"),
                        }),
                    }
                }
            }
            _ => Err(ParseError {
                diag: Diag::new("E0109", loc, "expected pattern"),
            }),
        }
    }

    // === Struct ===

    fn parse_struct_decl(&mut self, vis: Visibility) -> ParseResult<StructDecl> {
        let loc = self.loc();
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
            let f_name = self.parse_ident()?;
            let _ = self.expect(TokenKind::Colon)?;
            let f_type = self.parse_type()?;
            fields.push(StructField {
                name: f_name,
                type_: f_type,
                loc: self.loc(),
            });
            if !self.consume_if(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(StructDecl {
            name,
            fields,
            layout: None,
            visibility: vis,
            loc,
        })
    }

    // === Enum ===

    fn parse_enum_decl(&mut self, vis: Visibility) -> ParseResult<EnumDecl> {
        let loc = self.loc();
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
            let v_name = self.parse_ident()?;
            let mut fields = Vec::new();
            if self.check(&TokenKind::LParen) {
                self.advance();
                if !self.check(&TokenKind::RParen) {
                    fields.push(self.parse_type()?);
                    while self.consume_if(&TokenKind::Comma) {
                        fields.push(self.parse_type()?);
                    }
                }
                self.expect(TokenKind::RParen)?;
            }
            // Parse optional = value
            let value = if self.consume_if(&TokenKind::Eq) {
                match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(n),
                        ..
                    }) => Some(n),
                    _ => {
                        return Err(ParseError {
                            diag: Diag::new(
                                "E0112",
                                self.loc(),
                                "expected integer value after `=` in enum variant",
                            ),
                        });
                    }
                }
            } else {
                None
            };
            variants.push(EnumVariant {
                name: v_name,
                fields,
                value,
                loc: self.loc(),
            });
            if !self.consume_if(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(EnumDecl {
            name,
            variants,
            visibility: vis,
            loc,
        })
    }

    // === Type alias ===

    fn parse_type_alias(&mut self) -> ParseResult<TypeAlias> {
        let loc = self.loc();
        let name = self.parse_ident()?;
        let name_clone = name.clone();
        // .phor supports both "type Name = Type;" and "type Name : u32 = enum { ... }"
        if self.consume_if(&TokenKind::Colon) {
            // type Name : BaseType = ... — consume the base type
            // But also handle "type Name : struct { ... }" and "type Name : u32 = enum { ... }"
            if self.check(&TokenKind::Struct) {
                // type Name : struct { fields } — inline struct type
                self.advance();
                let _sdecl = self.parse_struct_decl_body(loc)?;
                return Ok(TypeAlias {
                    name: name_clone,
                    type_: TypeExpr::Named(name),
                    loc,
                });
            } else if self.check(&TokenKind::Enum) {
                // type Name : u32 = enum { ... } — but enum comes after =
                // Actually this is type Name : BaseType = enum { ... }
                // The colon type is the base type (e.g., u32)
                let _ = self.parse_type()?;
            } else if !self.check(&TokenKind::Eq) {
                let _ = self.parse_type()?;
            }
        }
        self.expect(TokenKind::Eq)?;
        // Check if RHS is an enum or struct literal inline
        if self.check(&TokenKind::Enum) {
            // type X = enum { ... } — parse inline enum
            self.advance(); // consume 'enum'
            self.parse_enum_decl_body(name.clone(), loc)
        } else if self.check(&TokenKind::Struct) {
            // type X : struct { ... } — parse inline struct
            self.advance(); // consume 'struct'
            let sdecl = self.parse_struct_decl_body(loc)?;
            Ok(TypeAlias {
                name,
                type_: TypeExpr::Named(sdecl.name),
                loc,
            })
        } else {
            let type_ = self.parse_type()?;
            self.consume_if(&TokenKind::Semi); // optional semicolon
            Ok(TypeAlias { name, type_, loc })
        }
    }

    fn parse_enum_decl_body(&mut self, name: Ident, loc: Loc) -> ParseResult<TypeAlias> {
        // Create a synthetic enum type from inline enum body
        self.expect(TokenKind::LBrace)?;
        let mut _variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
            let v_name = self.parse_ident()?;
            let fields = Vec::new();
            let value = if self.check(&TokenKind::Eq) {
                self.advance(); // skip "="
                match self.advance() {
                    Some(Token {
                        kind: TokenKind::Int(n),
                        ..
                    }) => Some(n),
                    _ => None,
                }
            } else {
                None
            };
            _variants.push(EnumVariant {
                name: v_name,
                fields,
                value,
                loc: self.loc(),
            });
            if !self.consume_if(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        // Semicolons in .phor type aliases ending with brace may have no semicolon
        self.consume_if(&TokenKind::Semi);
        Ok(TypeAlias {
            name,
            type_: TypeExpr::Prim(PrimType::U64, loc),
            loc,
        })
    }

    fn parse_struct_decl_body(&mut self, loc: Loc) -> ParseResult<StructDecl> {
        let name = Ident {
            name: "_anon".to_string(),
            loc,
        };
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && self.peek().is_some() {
            let f_name = self.parse_ident()?;
            let _ = self.expect(TokenKind::Colon)?;
            let f_type = self.parse_type()?;
            fields.push(StructField {
                name: f_name,
                type_: f_type,
                loc: self.loc(),
            });
            if !self.consume_if(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.consume_if(&TokenKind::Semi);
        Ok(StructDecl {
            name,
            fields,
            layout: None,
            visibility: Visibility::Private,
            loc,
        })
    }
}

/// Check if a keyword token can appear in identifier position
fn is_ident_like_keyword(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn
            | TokenKind::Trust
            | TokenKind::Trusted
            | TokenKind::Residual
            | TokenKind::Sealed
            | TokenKind::Dialect
            | TokenKind::Cage
            | TokenKind::Bound
            | TokenKind::Loop
            | TokenKind::Machine
            | TokenKind::Court
            | TokenKind::Oracle
            | TokenKind::Provenance
            | TokenKind::Receipt
            | TokenKind::TrustState
            | TokenKind::ResidualTy
            | TokenKind::Cap
            | TokenKind::Handle
            | TokenKind::Effect
            | TokenKind::Generation
            | TokenKind::Profile
            | TokenKind::NoStd
            | TokenKind::NoAlloc
            | TokenKind::NoUnsafe
            | TokenKind::Mut
            | TokenKind::In
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Union
            | TokenKind::Trait
            | TokenKind::Type
            | TokenKind::Const
            | TokenKind::Static
            | TokenKind::Pub
            | TokenKind::Use
            | TokenKind::Import
            | TokenKind::Export
            | TokenKind::For
            | TokenKind::While
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Match
            | TokenKind::Return
            | TokenKind::Yield
            | TokenKind::Spawn
            | TokenKind::Ref
            | TokenKind::As
    )
}

/// Convert a keyword token to its string representation
fn keyword_to_string(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Fn => "fn",
        TokenKind::Trust => "trust",
        TokenKind::Trusted => "trusted",
        TokenKind::Residual => "residual",
        TokenKind::Sealed => "sealed",
        TokenKind::Dialect => "dialect",
        TokenKind::Cage => "cage",
        TokenKind::Bound => "bound",
        TokenKind::Loop => "loop",
        TokenKind::Machine => "machine",
        TokenKind::Court => "court",
        TokenKind::Oracle => "oracle",
        TokenKind::Cap => "cap",
        TokenKind::Handle => "handle",
        TokenKind::Effect => "effect",
        TokenKind::Struct => "struct",
        TokenKind::Enum => "enum",
        TokenKind::Union => "union",
        TokenKind::Trait => "trait",
        TokenKind::Type => "type",
        TokenKind::Const => "const",
        TokenKind::Static => "static",
        TokenKind::Pub => "pub",
        TokenKind::Use => "use",
        TokenKind::Import => "import",
        TokenKind::Export => "export",
        TokenKind::Mut => "mut",
        TokenKind::In => "in",
        TokenKind::For => "for",
        TokenKind::While => "while",
        TokenKind::If => "if",
        TokenKind::Else => "else",
        TokenKind::Match => "match",
        TokenKind::Return => "return",
        TokenKind::Yield => "yield",
        TokenKind::Spawn => "spawn",
        TokenKind::Ref => "ref",
        TokenKind::As => "as",
        TokenKind::Provenance => "Provenance",
        TokenKind::Receipt => "Receipt",
        TokenKind::TrustState => "TrustState",
        TokenKind::ResidualTy => "ResidualTy",
        TokenKind::Generation => "Generation",
        TokenKind::Profile => "Profile",
        TokenKind::NoStd => "no_std",
        TokenKind::NoAlloc => "no_alloc",
        TokenKind::NoUnsafe => "no_unsafe",
        _ => "keyword",
    }
    .to_string()
}
