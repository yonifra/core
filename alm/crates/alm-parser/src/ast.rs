//! ALM Abstract Syntax Tree types.

use alm_lexer::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Bind(Bind),
    Annotation(Annotation),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Bind {
    pub name: String,
    pub stack: bool, // := vs =
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<Expr>,
    pub body: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Literal(Literal, Span),

    // Identifiers
    Ident(String, Span),

    // Environment reference: $VAR
    EnvRef(String, Span),

    // Block: { stmts }
    Block(Vec<Stmt>, Span),

    // Function call: f(args)
    Call(Box<Expr>, Vec<Expr>, Span),

    // Field access: expr.field
    Field(Box<Expr>, String, Span),

    // Struct literal: Name{field: val, ...}
    StructLit(String, Vec<(String, Expr)>, Span),

    // Lambda: (params) { body }
    Lambda(Vec<String>, Box<Expr>, Span),

    // Unary operators
    Async(Box<Expr>, Span),       // ~expr
    Effect(Box<Expr>, Span),      // !expr
    Metric(Box<Expr>, Span),      // #expr
    Return(Box<Expr>, Span),      // >expr
    Borrow(Box<Expr>, bool, Span),// &expr (mut=false) or &*expr (mut=true)
    Move(Box<Expr>, Span),        // ^expr
    Deref(Box<Expr>, Span),       // *expr

    // Postfix operators
    Try(Box<Expr>, Span),         // expr?
    Elvis(Box<Expr>, Box<Expr>, Span), // expr?: default
    Increment(Box<Expr>, Span),   // expr++

    // Match: expr | pat => body | pat => body
    Match(Box<Expr>, Vec<MatchArm>, Span),

    // Loop: *(cond){body}
    Loop(Option<Box<Expr>>, Box<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(_, s) | Expr::Ident(_, s) | Expr::EnvRef(_, s)
            | Expr::Block(_, s) | Expr::Call(_, _, s) | Expr::Field(_, _, s)
            | Expr::StructLit(_, _, s) | Expr::Lambda(_, _, s)
            | Expr::Async(_, s) | Expr::Effect(_, s) | Expr::Metric(_, s)
            | Expr::Return(_, s) | Expr::Borrow(_, _, s) | Expr::Move(_, s)
            | Expr::Deref(_, s) | Expr::Try(_, s) | Expr::Elvis(_, _, s)
            | Expr::Increment(_, s) | Expr::Match(_, _, s) | Expr::Loop(_, _, s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Span),                // _
    Error(Span),                   // ?
    Literal(Literal, Span),        // 42, "str", true
    Ident(String, Span),           // name (binding)
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
}
