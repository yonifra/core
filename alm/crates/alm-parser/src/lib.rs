//! ALM Parser — LL(1) recursive descent parser producing typed AST.
//!
//! Grammar (core subset for alpha):
//!   program     = stmt*
//!   stmt        = annotation | binding | expr_stmt
//!   annotation  = '@' IDENT '(' args ')' block?
//!   binding     = IDENT '=' expr ';'
//!   stack_bind  = IDENT ':=' expr ';'
//!   expr_stmt   = expr ';'
//!   expr        = match_expr
//!   match_expr  = pipe_expr ('|' pattern '=>' expr)*
//!   pipe_expr   = unary (('?' | '?:' expr | '++')?)
//!   unary       = ('~' | '!' | '&' | '&*' | '^' | '#' | '>' | '*') unary | call_expr
//!   call_expr   = primary ('(' args ')')* ('.' IDENT)*
//!   primary     = IDENT | INT | FLOAT | STR | BOOL | '(' expr ')' | block | struct_lit
//!   block       = '{' stmt* expr? '}'
//!   struct_lit  = IDENT '{' (IDENT ':' expr ',')* '}'
//!   args        = (expr (',' expr)*)?

pub mod ast;

use alm_lexer::{Lexer, Token, TokenKind, Span, LexError};
use ast::*;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub msg: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E100 parse error at {}:{}: {}", self.span.line, self.span.col, self.msg)
    }
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError { msg: e.msg, span: e.span }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(source: &str) -> Result<Self, ParseError> {
        let tokens = Lexer::tokenize(source)?;
        Ok(Self { tokens, pos: 0 })
    }

    fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn peek_span(&self) -> Span {
        self.tokens.get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(0, 0, 0, 0))
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline | TokenKind::Comment(_)) {
            self.advance();
        }
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<&Token, ParseError> {
        self.skip_newlines();
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            Ok(self.advance())
        } else {
            Err(ParseError {
                msg: format!("expected {expected}, got {}", self.peek()),
                span: self.peek_span(),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        self.skip_newlines();
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            Ok(name)
        } else {
            Err(ParseError {
                msg: format!("expected identifier, got {}", self.peek()),
                span: self.peek_span(),
            })
        }
    }

    /// Parse an entire program.
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        Ok(Program { stmts })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        let span = self.peek_span();

        match self.peek() {
            TokenKind::At => {
                let ann = self.parse_annotation()?;
                self.consume_semi();
                Ok(ann)
            }

            TokenKind::Ident(_) => {
                // Look ahead: IDENT '=' or IDENT ':=' → binding
                let saved = self.pos;
                let name = self.expect_ident()?;
                self.skip_newlines();

                match self.peek() {
                    TokenKind::Eq => {
                        self.advance();
                        let expr = self.parse_expr()?;
                        self.consume_semi();
                        Ok(Stmt::Bind(Bind { name, stack: false, expr, span }))
                    }
                    TokenKind::ColonEq => {
                        self.advance();
                        let expr = self.parse_expr()?;
                        self.consume_semi();
                        Ok(Stmt::Bind(Bind { name, stack: true, expr, span }))
                    }
                    _ => {
                        // Backtrack — it's an expression starting with ident
                        self.pos = saved;
                        let expr = self.parse_expr()?;
                        self.consume_semi();
                        Ok(Stmt::Expr(expr))
                    }
                }
            }

            _ => {
                let expr = self.parse_expr()?;
                self.consume_semi();
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn consume_semi(&mut self) {
        self.skip_newlines();
        if matches!(self.peek(), TokenKind::Semi) {
            self.advance();
        }
    }

    fn parse_annotation(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&TokenKind::At)?;
        let name = self.expect_ident()?;

        let args = if matches!(self.peek(), TokenKind::LParen) {
            self.advance();
            let args = self.parse_args()?;
            self.expect(&TokenKind::RParen)?;
            args
        } else {
            vec![]
        };

        self.skip_newlines();
        let body = if matches!(self.peek(), TokenKind::LBrace) {
            Some(Box::new(self.parse_block()?))
        } else {
            None
        };

        Ok(Stmt::Annotation(Annotation { name, args, body, span }))
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RParen) {
            return Ok(args);
        }

        args.push(self.parse_expr()?);
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            self.skip_newlines();
            args.push(self.parse_expr()?);
        }
        Ok(args)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let expr = self.parse_postfix()?;

        // Match arms: expr | pattern => body | pattern => body
        if matches!(self.peek(), TokenKind::Pipe) {
            let mut arms = Vec::new();
            while matches!(self.peek(), TokenKind::Pipe) {
                self.advance(); // consume |
                self.skip_newlines();
                let pattern = self.parse_pattern()?;
                self.expect(&TokenKind::FatArrow)?;
                let body = self.parse_postfix()?;
                arms.push(MatchArm { pattern, body });
            }
            let span = expr.span();
            return Ok(Expr::Match(Box::new(expr), arms, span));
        }

        Ok(expr)
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.skip_newlines();
        match self.peek().clone() {
            TokenKind::Under => {
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Wildcard(span))
            }
            TokenKind::Question => {
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Error(span))
            }
            TokenKind::Int(v) => {
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Literal(Literal::Int(v), span))
            }
            TokenKind::Str(ref s) => {
                let s = s.clone();
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Literal(Literal::Str(s), span))
            }
            TokenKind::Bool(v) => {
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Literal(Literal::Bool(v), span))
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                let span = self.peek_span();
                self.advance();
                Ok(Pattern::Ident(name, span))
            }
            _ => Err(ParseError {
                msg: format!("expected pattern, got {}", self.peek()),
                span: self.peek_span(),
            }),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;

        loop {
            self.skip_newlines();
            match self.peek() {
                TokenKind::Question => {
                    let span = self.peek_span();
                    self.advance();
                    // Check for ?: (elvis)
                    if matches!(self.peek(), TokenKind::Colon) {
                        self.advance();
                        let default = self.parse_unary()?;
                        expr = Expr::Elvis(Box::new(expr), Box::new(default), span);
                    } else {
                        expr = Expr::Try(Box::new(expr), span);
                    }
                }
                TokenKind::PlusPlus => {
                    let span = self.peek_span();
                    self.advance();
                    expr = Expr::Increment(Box::new(expr), span);
                }
                TokenKind::LParen => {
                    self.advance();
                    let args = self.parse_args()?;
                    self.expect(&TokenKind::RParen)?;
                    let span = expr.span();
                    expr = Expr::Call(Box::new(expr), args, span);
                }
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    let span = expr.span();
                    expr = Expr::Field(Box::new(expr), field, span);
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let span = self.peek_span();

        match self.peek() {
            TokenKind::Tilde => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Async(Box::new(inner), span))
            }
            TokenKind::Bang => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Effect(Box::new(inner), span))
            }
            TokenKind::Hash => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Metric(Box::new(inner), span))
            }
            TokenKind::Gt => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Return(Box::new(inner), span))
            }
            TokenKind::Amp => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Borrow(Box::new(inner), false, span))
            }
            TokenKind::AmpStar => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Borrow(Box::new(inner), true, span))
            }
            TokenKind::Caret => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Move(Box::new(inner), span))
            }
            TokenKind::Star => {
                self.advance();
                // Could be loop: *(expr){body} or deref
                if matches!(self.peek(), TokenKind::LParen) {
                    self.advance();
                    let cond = if matches!(self.peek(), TokenKind::RParen) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(&TokenKind::RParen)?;
                    let body = Box::new(self.parse_block()?);
                    Ok(Expr::Loop(cond, body, span))
                } else {
                    let inner = self.parse_unary()?;
                    Ok(Expr::Deref(Box::new(inner), span))
                }
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let span = self.peek_span();

        match self.peek().clone() {
            TokenKind::Int(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Int(v), span))
            }
            TokenKind::Float(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(v), span))
            }
            TokenKind::Str(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::Str(s), span))
            }
            TokenKind::Bool(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(v), span))
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                self.advance();
                // Check for struct literal: Name { ... }
                if matches!(self.peek(), TokenKind::LBrace) {
                    // Disambiguate: struct lit vs block
                    // Struct lit: IDENT '{' IDENT ':' ...
                    let saved = self.pos;
                    self.advance(); // consume {
                    self.skip_newlines();
                    if let TokenKind::Ident(_) = self.peek() {
                        let saved2 = self.pos;
                        self.advance();
                        if matches!(self.peek(), TokenKind::Colon) {
                            // It's a struct literal
                            self.pos = saved;
                            return self.parse_struct_lit(name, span);
                        }
                        self.pos = saved2;
                    }
                    self.pos = saved;
                }
                Ok(Expr::Ident(name, span))
            }
            TokenKind::Dollar => {
                self.advance();
                let name = self.expect_ident()?;
                Ok(Expr::EnvRef(name, span))
            }
            TokenKind::LParen => {
                self.advance();
                // Could be: empty parens (unit), single arg (grouping), or param list for lambda
                self.skip_newlines();
                if matches!(self.peek(), TokenKind::RParen) {
                    self.advance();
                    return Ok(Expr::Literal(Literal::Unit, span));
                }

                // Try to parse as lambda params: (a, b) { body }
                let saved = self.pos;
                if let Ok(params) = self.try_parse_params() {
                    if matches!(self.peek(), TokenKind::LBrace) {
                        let body = self.parse_block()?;
                        return Ok(Expr::Lambda(params, Box::new(body), span));
                    }
                }
                // Backtrack — it's a grouped expression
                self.pos = saved;
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::LBrace => {
                self.parse_block()
            }
            _ => Err(ParseError {
                msg: format!("expected expression, got {}", self.peek()),
                span: self.peek_span(),
            }),
        }
    }

    fn try_parse_params(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();
        self.skip_newlines();

        if let TokenKind::Ident(ref name) = self.peek().clone() {
            params.push(name.clone());
            self.advance();
        } else {
            return Err(ParseError { msg: "expected param".into(), span: self.peek_span() });
        }

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            self.skip_newlines();
            let name = self.expect_ident()?;
            params.push(name);
        }

        self.expect(&TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_struct_lit(&mut self, name: String, span: Span) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();

        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let field_name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));

            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::StructLit(name, fields, span))
    }

    fn parse_block(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek_span();
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();

        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Block(stmts, span))
    }

    /// Convenience: parse source string into AST.
    pub fn parse(source: &str) -> Result<Program, ParseError> {
        let mut parser = Parser::new(source)?;
        parser.parse_program()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_binding() {
        let prog = Parser::parse("x = 42;").unwrap();
        assert_eq!(prog.stmts.len(), 1);
        match &prog.stmts[0] {
            Stmt::Bind(b) => {
                assert_eq!(b.name, "x");
                assert!(!b.stack);
            }
            _ => panic!("expected binding"),
        }
    }

    #[test]
    fn test_stack_binding() {
        let prog = Parser::parse("x := 42;").unwrap();
        match &prog.stmts[0] {
            Stmt::Bind(b) => {
                assert_eq!(b.name, "x");
                assert!(b.stack);
            }
            _ => panic!("expected stack binding"),
        }
    }

    #[test]
    fn test_annotation() {
        let prog = Parser::parse("@test(myTest) { x = 1; }").unwrap();
        match &prog.stmts[0] {
            Stmt::Annotation(a) => {
                assert_eq!(a.name, "test");
                assert!(a.body.is_some());
            }
            _ => panic!("expected annotation"),
        }
    }

    #[test]
    fn test_function_call() {
        let prog = Parser::parse("proc(x, y);").unwrap();
        assert_eq!(prog.stmts.len(), 1);
    }

    #[test]
    fn test_try_operator() {
        let prog = Parser::parse("proc(x)?;").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::Try(_, _)) => {}
            other => panic!("expected Try, got {other:?}"),
        }
    }

    #[test]
    fn test_metric() {
        let prog = Parser::parse("#rq;").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::Metric(_, _)) => {}
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn test_return() {
        let prog = Parser::parse(">42;").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::Return(_, _)) => {}
            other => panic!("expected Return, got {other:?}"),
        }
    }

    #[test]
    fn test_alm_http_service() {
        // The canonical ALM example
        let src = r#"@svc(8080){#rq;p=proc(r)?;p|?=>retry(3,proc,r);>p}"#;
        let prog = Parser::parse(src).unwrap();
        assert!(!prog.stmts.is_empty());
    }

    #[test]
    fn test_struct_literal() {
        let prog = Parser::parse("w = Widget{a: 1, b: 2};").unwrap();
        match &prog.stmts[0] {
            Stmt::Bind(b) => {
                assert_eq!(b.name, "w");
                match &b.expr {
                    Expr::StructLit(name, fields, _) => {
                        assert_eq!(name, "Widget");
                        assert_eq!(fields.len(), 2);
                    }
                    other => panic!("expected StructLit, got {other:?}"),
                }
            }
            _ => panic!("expected binding"),
        }
    }

    #[test]
    fn test_match_expr() {
        let prog = Parser::parse("x | 1 => a | _ => b;").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::Match(_, arms, _)) => {
                assert_eq!(arms.len(), 2);
            }
            other => panic!("expected Match, got {other:?}"),
        }
    }

    #[test]
    fn test_loop() {
        let prog = Parser::parse("*(3){x;};").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::Loop(Some(_), _, _)) => {}
            other => panic!("expected Loop, got {other:?}"),
        }
    }

    #[test]
    fn test_env_ref() {
        let prog = Parser::parse("$HOME;").unwrap();
        match &prog.stmts[0] {
            Stmt::Expr(Expr::EnvRef(name, _)) => assert_eq!(name, "HOME"),
            other => panic!("expected EnvRef, got {other:?}"),
        }
    }
}
