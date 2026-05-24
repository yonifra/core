//! ALM Linter — static analysis for ALM source.
//!
//! Checks:
//! - W001: Identifier >12 chars (token efficiency warning)
//! - W002: snake_case identifier (should be camelCase)
//! - W003: Unused binding
//! - W004: Empty block
//! - W005: Metric without name
//! - E101: Invalid annotation name

use alm_lexer::{Lexer, TokenKind};
use alm_parser::ast::*;
use alm_parser::Parser;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct LintDiag {
    pub code: String,
    pub level: Level,
    pub msg: String,
    pub line: u32,
    pub col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

impl std::fmt::Display for LintDiag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.level {
            Level::Warning => "warn",
            Level::Error => "error",
        };
        write!(f, "{} {} at {}:{}: {}", self.code, level, self.line, self.col, self.msg)
    }
}

const VALID_ANNOTATIONS: &[&str] = &["test", "deploy", "svc", "bench", "doc", "cfg"];

pub struct Linter {
    diags: Vec<LintDiag>,
    strict: bool,
}

impl Linter {
    pub fn new(strict: bool) -> Self {
        Self { diags: Vec::new(), strict }
    }

    /// Lint ALM source. Returns diagnostics.
    pub fn lint(source: &str, strict: bool) -> Vec<LintDiag> {
        let mut linter = Linter::new(strict);
        linter.lint_tokens(source);
        if let Ok(prog) = Parser::parse(source) {
            linter.lint_program(&prog);
        }
        linter.diags
    }

    fn warn(&mut self, code: &str, line: u32, col: u16, msg: String) {
        self.diags.push(LintDiag {
            code: code.into(),
            level: Level::Warning,
            msg,
            line,
            col,
        });
    }

    fn error(&mut self, code: &str, line: u32, col: u16, msg: String) {
        self.diags.push(LintDiag {
            code: code.into(),
            level: Level::Error,
            msg,
            line,
            col,
        });
    }

    fn lint_tokens(&mut self, source: &str) {
        let tokens = match Lexer::tokenize(source) {
            Ok(t) => t,
            Err(_) => return,
        };

        for tok in &tokens {
            if let TokenKind::Ident(name) = &tok.kind {
                // W001: long identifier
                if name.len() > 12 {
                    self.warn("W001", tok.span.line, tok.span.col,
                        format!("identifier '{name}' exceeds 12 chars ({} chars) — reduces token efficiency", name.len()));
                }

                // W002: snake_case
                if name.contains('_') && name.chars().any(|c| c.is_lowercase()) {
                    self.warn("W002", tok.span.line, tok.span.col,
                        format!("'{name}' uses snake_case — camelCase is more token-efficient"));
                }
            }
        }
    }

    fn lint_program(&mut self, prog: &Program) {
        let mut defined: HashMap<String, (u32, u16)> = HashMap::new();
        let mut used: HashSet<String> = HashSet::new();

        for stmt in &prog.stmts {
            self.lint_stmt(stmt, &mut defined, &mut used);
        }

        // W003: unused bindings
        for (name, (line, col)) in &defined {
            if !used.contains(name) {
                self.warn("W003", *line, *col,
                    format!("binding '{name}' is never used"));
            }
        }
    }

    fn lint_stmt(&mut self, stmt: &Stmt, defined: &mut HashMap<String, (u32, u16)>, used: &mut HashSet<String>) {
        match stmt {
            Stmt::Bind(b) => {
                defined.insert(b.name.clone(), (b.span.line, b.span.col));
                self.lint_expr(&b.expr, used);
            }
            Stmt::Annotation(ann) => {
                // E101: invalid annotation
                if !VALID_ANNOTATIONS.contains(&ann.name.as_str()) && self.strict {
                    self.error("E101", ann.span.line, ann.span.col,
                        format!("unknown annotation '@{}'", ann.name));
                }
                for arg in &ann.args {
                    self.lint_expr(arg, used);
                }
                if let Some(body) = &ann.body {
                    self.lint_expr(body, used);
                }
            }
            Stmt::Expr(e) => {
                self.lint_expr(e, used);
            }
        }
    }

    fn lint_expr(&mut self, expr: &Expr, used: &mut HashSet<String>) {
        match expr {
            Expr::Ident(name, _) => { used.insert(name.clone()); }
            Expr::Block(stmts, span) => {
                if stmts.is_empty() {
                    self.warn("W004", span.line, span.col, "empty block".into());
                }
                for s in stmts {
                    self.lint_stmt(s, &mut HashMap::new(), used);
                }
            }
            Expr::Call(f, args, _) => {
                self.lint_expr(f, used);
                for a in args { self.lint_expr(a, used); }
            }
            Expr::Field(obj, _, _) => self.lint_expr(obj, used),
            Expr::StructLit(_, fields, _) => {
                for (_, v) in fields { self.lint_expr(v, used); }
            }
            Expr::Lambda(_, body, _) => self.lint_expr(body, used),
            Expr::Async(inner, _) | Expr::Effect(inner, _) | Expr::Return(inner, _)
            | Expr::Borrow(inner, _, _) | Expr::Move(inner, _) | Expr::Deref(inner, _)
            | Expr::Try(inner, _) | Expr::Increment(inner, _) => {
                self.lint_expr(inner, used);
            }
            Expr::Metric(inner, span) => {
                if let Expr::Ident(_, _) = inner.as_ref() {
                    self.lint_expr(inner, used);
                } else {
                    self.warn("W005", span.line, span.col, "metric should reference a named identifier".into());
                }
            }
            Expr::Elvis(a, b, _) => { self.lint_expr(a, used); self.lint_expr(b, used); }
            Expr::Match(scrut, arms, _) => {
                self.lint_expr(scrut, used);
                for arm in arms { self.lint_expr(&arm.body, used); }
            }
            Expr::Loop(cond, body, _) => {
                if let Some(c) = cond { self.lint_expr(c, used); }
                self.lint_expr(body, used);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_identifier() {
        let diags = Linter::lint("veryLongIdentifierName = 42", false);
        assert!(diags.iter().any(|d| d.code == "W001"));
    }

    #[test]
    fn test_snake_case() {
        let diags = Linter::lint("my_var = 42", false);
        assert!(diags.iter().any(|d| d.code == "W002"));
    }

    #[test]
    fn test_unused_binding() {
        let diags = Linter::lint("x = 42", false);
        assert!(diags.iter().any(|d| d.code == "W003"));
    }

    #[test]
    fn test_used_binding_no_warning() {
        let diags = Linter::lint("x = 42; x", false);
        assert!(!diags.iter().any(|d| d.code == "W003"));
    }

    #[test]
    fn test_empty_block() {
        let diags = Linter::lint("{}", false);
        assert!(diags.iter().any(|d| d.code == "W004"));
    }

    #[test]
    fn test_invalid_annotation_strict() {
        let diags = Linter::lint("@foobar(x) { 1 }", true);
        assert!(diags.iter().any(|d| d.code == "E101"));
    }

    #[test]
    fn test_valid_annotation_no_error() {
        let diags = Linter::lint("@test(x) { assert(true) }", true);
        assert!(!diags.iter().any(|d| d.code == "E101"));
    }

    #[test]
    fn test_camel_case_ok() {
        let diags = Linter::lint("myVar = 42; myVar", false);
        assert!(!diags.iter().any(|d| d.code == "W002"));
    }

    #[test]
    fn test_clean_code() {
        let diags = Linter::lint("x = 42; x", false);
        assert!(diags.is_empty(), "clean code should have no warnings, got: {diags:?}");
    }
}
