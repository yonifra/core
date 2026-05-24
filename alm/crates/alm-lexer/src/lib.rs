//! ALM Lexer — zero-copy, streaming tokenizer for ALM syntax.
//!
//! Produces a stream of `Token` values from ALM source text.
//! All 24 structural tokens are single-char (BPE-aligned).
//! No keywords — only sigils, identifiers, and literals.

use std::fmt;

/// Source location for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub offset: u32,
    pub len: u16,
    pub line: u32,
    pub col: u16,
}

impl Span {
    pub fn new(offset: u32, len: u16, line: u32, col: u16) -> Self {
        Self { offset, len, line, col }
    }
}

/// All token kinds in ALM. 24 structural + identifiers + literals + EOF.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Structural tokens (24 sigils)
    LBrace,    // {
    RBrace,    // }
    LParen,    // (
    RParen,    // )
    LBracket,  // [
    RBracket,  // ]
    Colon,     // :
    Semi,      // ;
    Dot,       // .
    Comma,     // ,
    Eq,        // =
    Gt,        // >
    Lt,        // <
    Pipe,      // |
    At,        // @
    Hash,      // #
    Bang,      // !
    Question,  // ?
    Tilde,     // ~
    Dollar,    // $
    Caret,     // ^
    Amp,       // &
    Star,      // *
    Under,     // _

    // Multi-char operators
    DotDot,    // ..
    FatArrow,  // =>
    ColonEq,   // :=
    AmpStar,   // &*  (mutable borrow)
    QuestionColon, // ?: (elvis)
    EqEq,      // == (equality)
    BangEq,    // != (not equal)
    GtEq,      // >=
    LtEq,      // <=
    PlusPlus,  // ++ (increment, used with #metric++)
    Arrow,     // -> (alt data flow)
    LtDash,    // <- (draw from)

    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),

    // Identifiers
    Ident(String),

    // Misc
    Comment(String), // // ...
    Newline,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LBrace => write!(f, "{{"),
            Self::RBrace => write!(f, "}}"),
            Self::LParen => write!(f, "("),
            Self::RParen => write!(f, ")"),
            Self::LBracket => write!(f, "["),
            Self::RBracket => write!(f, "]"),
            Self::Colon => write!(f, ":"),
            Self::Semi => write!(f, ";"),
            Self::Dot => write!(f, "."),
            Self::Comma => write!(f, ","),
            Self::Eq => write!(f, "="),
            Self::Gt => write!(f, ">"),
            Self::Lt => write!(f, "<"),
            Self::Pipe => write!(f, "|"),
            Self::At => write!(f, "@"),
            Self::Hash => write!(f, "#"),
            Self::Bang => write!(f, "!"),
            Self::Question => write!(f, "?"),
            Self::Tilde => write!(f, "~"),
            Self::Dollar => write!(f, "$"),
            Self::Caret => write!(f, "^"),
            Self::Amp => write!(f, "&"),
            Self::Star => write!(f, "*"),
            Self::Under => write!(f, "_"),
            Self::DotDot => write!(f, ".."),
            Self::FatArrow => write!(f, "=>"),
            Self::ColonEq => write!(f, ":="),
            Self::AmpStar => write!(f, "&*"),
            Self::QuestionColon => write!(f, "?:"),
            Self::EqEq => write!(f, "=="),
            Self::BangEq => write!(f, "!="),
            Self::GtEq => write!(f, ">="),
            Self::LtEq => write!(f, "<="),
            Self::PlusPlus => write!(f, "++"),
            Self::Arrow => write!(f, "->"),
            Self::LtDash => write!(f, "<-"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Str(v) => write!(f, "\"{v}\""),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Ident(v) => write!(f, "{v}"),
            Self::Comment(v) => write!(f, "//{v}"),
            Self::Newline => write!(f, "\\n"),
            Self::Eof => write!(f, "EOF"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub msg: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E000 lex error at {}:{}: {}", self.span.line, self.span.col, self.msg)
    }
}

/// Zero-copy streaming lexer for ALM source.
pub struct Lexer<'src> {
    src: &'src [u8],
    pos: usize,
    line: u32,
    col: u16,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            src: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.src.get(self.pos).copied()?;
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn span_from(&self, start_offset: usize, start_line: u32, start_col: u16) -> Span {
        Span::new(
            start_offset as u32,
            (self.pos - start_offset) as u16,
            start_line,
            start_col,
        )
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == b' ' || ch == b'\t' || ch == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn lex_string(&mut self, start_offset: usize, start_line: u32, start_col: u16) -> Result<Token, LexError> {
        // Opening quote already consumed
        let mut s = String::new();
        loop {
            match self.advance() {
                None => {
                    return Err(LexError {
                        msg: "unterminated string".into(),
                        span: self.span_from(start_offset, start_line, start_col),
                    });
                }
                Some(b'"') => break,
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'"') => s.push('"'),
                        Some(c) => {
                            return Err(LexError {
                                msg: format!("invalid escape: \\{}", c as char),
                                span: self.span_from(start_offset, start_line, start_col),
                            });
                        }
                        None => {
                            return Err(LexError {
                                msg: "unterminated escape".into(),
                                span: self.span_from(start_offset, start_line, start_col),
                            });
                        }
                    }
                }
                Some(c) => s.push(c as char),
            }
        }
        Ok(Token {
            kind: TokenKind::Str(s),
            span: self.span_from(start_offset, start_line, start_col),
        })
    }

    fn lex_number(&mut self, first: u8, start_offset: usize, start_line: u32, start_col: u16, negative: bool) -> Result<Token, LexError> {
        let mut num_str = String::new();
        if negative {
            num_str.push('-');
        }
        num_str.push(first as char);
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch as char);
                self.advance();
            } else if ch == b'.' && self.peek2().is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                num_str.push('.');
                self.advance();
            } else {
                break;
            }
        }

        let span = self.span_from(start_offset, start_line, start_col);
        if is_float {
            match num_str.parse::<f64>() {
                Ok(v) => Ok(Token { kind: TokenKind::Float(v), span }),
                Err(_) => Err(LexError { msg: format!("invalid float: {num_str}"), span }),
            }
        } else {
            match num_str.parse::<i64>() {
                Ok(v) => Ok(Token { kind: TokenKind::Int(v), span }),
                Err(_) => Err(LexError { msg: format!("invalid int: {num_str}"), span }),
            }
        }
    }

    fn lex_ident(&mut self, first: u8, start_offset: usize, start_line: u32, start_col: u16) -> Token {
        let mut name = String::new();
        name.push(first as char);

        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                name.push(ch as char);
                self.advance();
            } else {
                break;
            }
        }

        let span = self.span_from(start_offset, start_line, start_col);

        // Only two "keyword-like" identifiers: true/false → Bool literals
        let kind = match name.as_str() {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => TokenKind::Ident(name),
        };

        Token { kind, span }
    }

    /// Lex the next token from the source.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();

        let so = self.pos;   // start offset
        let sl = self.line;  // start line
        let sc = self.col;   // start col

        let ch = match self.advance() {
            Some(ch) => ch,
            None => {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(so as u32, 0, sl, sc),
                });
            }
        };

        // Helper macro — builds Token with current pos for span end
        macro_rules! tok {
            ($kind:expr) => {
                Ok(Token {
                    kind: $kind,
                    span: Span::new(so as u32, (self.pos - so) as u16, sl, sc),
                })
            };
        }

        match ch {
            b'{' => tok!(TokenKind::LBrace),
            b'}' => tok!(TokenKind::RBrace),
            b'(' => tok!(TokenKind::LParen),
            b')' => tok!(TokenKind::RParen),
            b'[' => tok!(TokenKind::LBracket),
            b']' => tok!(TokenKind::RBracket),
            b';' => tok!(TokenKind::Semi),
            b',' => tok!(TokenKind::Comma),
            b'~' => tok!(TokenKind::Tilde),
            b'^' => tok!(TokenKind::Caret),
            b'\n' => tok!(TokenKind::Newline),

            b':' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    tok!(TokenKind::ColonEq)
                } else {
                    tok!(TokenKind::Colon)
                }
            }

            b'.' => {
                if self.peek() == Some(b'.') {
                    self.advance();
                    tok!(TokenKind::DotDot)
                } else {
                    tok!(TokenKind::Dot)
                }
            }

            b'=' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    tok!(TokenKind::FatArrow)
                } else if self.peek() == Some(b'=') {
                    self.advance();
                    tok!(TokenKind::EqEq)
                } else {
                    tok!(TokenKind::Eq)
                }
            }

            b'>' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    tok!(TokenKind::GtEq)
                } else {
                    tok!(TokenKind::Gt)
                }
            }

            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    tok!(TokenKind::LtEq)
                } else if self.peek() == Some(b'-') {
                    self.advance();
                    tok!(TokenKind::LtDash)
                } else {
                    tok!(TokenKind::Lt)
                }
            }

            b'|' => tok!(TokenKind::Pipe),

            b'@' => tok!(TokenKind::At),

            b'#' => tok!(TokenKind::Hash),

            b'!' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    tok!(TokenKind::BangEq)
                } else {
                    tok!(TokenKind::Bang)
                }
            }

            b'?' => {
                if self.peek() == Some(b':') {
                    self.advance();
                    tok!(TokenKind::QuestionColon)
                } else {
                    tok!(TokenKind::Question)
                }
            }

            b'$' => tok!(TokenKind::Dollar),

            b'&' => {
                if self.peek() == Some(b'*') {
                    self.advance();
                    tok!(TokenKind::AmpStar)
                } else {
                    tok!(TokenKind::Amp)
                }
            }

            b'*' => tok!(TokenKind::Star),
            b'_' if !self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) => tok!(TokenKind::Under),

            b'+' => {
                if self.peek() == Some(b'+') {
                    self.advance();
                    tok!(TokenKind::PlusPlus)
                } else {
                    Err(LexError {
                        msg: "unexpected '+'".into(),
                        span: Span::new(so as u32, 1, sl, sc),
                    })
                }
            }

            b'-' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    tok!(TokenKind::Arrow)
                } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    let digit = self.advance().unwrap();
                    self.lex_number(digit, so, sl, sc, true)
                } else {
                    Err(LexError {
                        msg: "unexpected '-'".into(),
                        span: Span::new(so as u32, 1, sl, sc),
                    })
                }
            }

            b'/' => {
                if self.peek() == Some(b'/') {
                    self.advance();
                    let mut comment = String::new();
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        comment.push(c as char);
                        self.advance();
                    }
                    tok!(TokenKind::Comment(comment))
                } else {
                    Err(LexError {
                        msg: "unexpected '/'".into(),
                        span: Span::new(so as u32, 1, sl, sc),
                    })
                }
            }

            b'"' => self.lex_string(so, sl, sc),

            c if c.is_ascii_digit() => self.lex_number(c, so, sl, sc, false),

            c if c.is_ascii_alphabetic() || c == b'_' => Ok(self.lex_ident(c, so, sl, sc)),

            c => Err(LexError {
                msg: format!("unexpected character: '{}'", c as char),
                span: Span::new(so as u32, 1, sl, sc),
            }),
        }
    }

    /// Tokenize entire source into a Vec.
    pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let tok = lexer.next_token()?;
            if tok.kind == TokenKind::Eof {
                tokens.push(tok);
                break;
            }
            tokens.push(tok);
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structural_tokens() {
        let src = "{} () [] ; , . = > < | @ # ! ? ~ $ ^ & * _";
        let tokens = Lexer::tokenize(src).unwrap();
        let kinds: Vec<_> = tokens.iter()
            .filter(|t| !matches!(t.kind, TokenKind::Eof | TokenKind::Newline))
            .map(|t| &t.kind)
            .collect();
        assert_eq!(kinds[0], &TokenKind::LBrace);
        assert_eq!(kinds[1], &TokenKind::RBrace);
        assert_eq!(kinds[2], &TokenKind::LParen);
        assert_eq!(kinds[3], &TokenKind::RParen);
        assert_eq!(kinds[4], &TokenKind::LBracket);
        assert_eq!(kinds[5], &TokenKind::RBracket);
        assert_eq!(kinds[6], &TokenKind::Semi);
        assert_eq!(kinds[7], &TokenKind::Comma);
        assert_eq!(kinds[8], &TokenKind::Dot);
        assert_eq!(kinds[9], &TokenKind::Eq);
        assert_eq!(kinds[10], &TokenKind::Gt);
        assert_eq!(kinds[11], &TokenKind::Lt);
        assert_eq!(kinds[12], &TokenKind::Pipe);
        assert_eq!(kinds[13], &TokenKind::At);
        assert_eq!(kinds[14], &TokenKind::Hash);
        assert_eq!(kinds[15], &TokenKind::Bang);
        assert_eq!(kinds[16], &TokenKind::Question);
        assert_eq!(kinds[17], &TokenKind::Tilde);
        assert_eq!(kinds[18], &TokenKind::Dollar);
        assert_eq!(kinds[19], &TokenKind::Caret);
        assert_eq!(kinds[20], &TokenKind::Amp);
        assert_eq!(kinds[21], &TokenKind::Star);
        assert_eq!(kinds[22], &TokenKind::Under);
    }

    #[test]
    fn test_multi_char_operators() {
        let src = ".. => := &* ?: == != >= <= ++ -> <-";
        let tokens = Lexer::tokenize(src).unwrap();
        let kinds: Vec<_> = tokens.iter()
            .filter(|t| !matches!(t.kind, TokenKind::Eof))
            .map(|t| &t.kind)
            .collect();
        assert_eq!(kinds, vec![
            &TokenKind::DotDot,
            &TokenKind::FatArrow,
            &TokenKind::ColonEq,
            &TokenKind::AmpStar,
            &TokenKind::QuestionColon,
            &TokenKind::EqEq,
            &TokenKind::BangEq,
            &TokenKind::GtEq,
            &TokenKind::LtEq,
            &TokenKind::PlusPlus,
            &TokenKind::Arrow,
            &TokenKind::LtDash,
        ]);
    }

    #[test]
    fn test_literals() {
        let src = r#"42 3.14 "hello" true false"#;
        let tokens = Lexer::tokenize(src).unwrap();
        let kinds: Vec<_> = tokens.iter()
            .filter(|t| !matches!(t.kind, TokenKind::Eof))
            .map(|t| &t.kind)
            .collect();
        assert_eq!(kinds, vec![
            &TokenKind::Int(42),
            &TokenKind::Float(3.14),
            &TokenKind::Str("hello".into()),
            &TokenKind::Bool(true),
            &TokenKind::Bool(false),
        ]);
    }

    #[test]
    fn test_alm_http_service() {
        // The canonical ALM example from the SDD
        let src = r#"@svc(8080){~(r){#rq;p=proc(r)?;p|?=>retry(3,proc,r);>p}}"#;
        let tokens = Lexer::tokenize(src).unwrap();
        // Should parse without errors
        assert!(tokens.last().unwrap().kind == TokenKind::Eof);
        // Count non-EOF tokens
        let count = tokens.iter().filter(|t| !matches!(t.kind, TokenKind::Eof)).count();
        // Verify token density — should be around 30 tokens for this expression
        assert!(count < 40, "Token count {count} exceeds expected density");
    }

    #[test]
    fn test_comments() {
        let src = "x // this is a comment\ny";
        let tokens = Lexer::tokenize(src).unwrap();
        let has_comment = tokens.iter().any(|t| matches!(&t.kind, TokenKind::Comment(_)));
        assert!(has_comment);
    }

    #[test]
    fn test_string_escapes() {
        let src = r#""hello\nworld""#;
        let tokens = Lexer::tokenize(src).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Str("hello\nworld".into()));
    }

    #[test]
    fn test_negative_numbers() {
        let src = "-42";
        let tokens = Lexer::tokenize(src).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Int(-42));
    }

    #[test]
    fn test_span_tracking() {
        let src = "x=42";
        let tokens = Lexer::tokenize(src).unwrap();
        assert_eq!(tokens[0].span.col, 1);
        assert_eq!(tokens[1].span.col, 2);
        assert_eq!(tokens[2].span.col, 3);
    }
}
