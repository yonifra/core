//! Effect System — track and enforce side-effect restrictions.
//!
//! Effects tracked: !io, !net, !fs, !time, !panic
//! In sandbox:strict mode, code with forbidden effects is rejected at compile time.

use alm_parser::ast::*;
use alm_parser::Parser;
use std::collections::HashSet;

/// Known effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    Io,    // General I/O
    Net,   // Network access
    Fs,    // Filesystem access
    Time,  // System clock
    Panic, // Can panic/abort
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effect::Io => write!(f, "!io"),
            Effect::Net => write!(f, "!net"),
            Effect::Fs => write!(f, "!fs"),
            Effect::Time => write!(f, "!time"),
            Effect::Panic => write!(f, "!panic"),
        }
    }
}

/// Effect checking result.
#[derive(Debug, Clone)]
pub struct EffectReport {
    pub effects: HashSet<Effect>,
    pub violations: Vec<EffectViolation>,
}

#[derive(Debug, Clone)]
pub struct EffectViolation {
    pub effect: Effect,
    pub location: String,
    pub msg: String,
}

impl std::fmt::Display for EffectViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E250 effect violation at {}: {} — {}", self.location, self.effect, self.msg)
    }
}

/// Functions known to have effects.
const IO_FUNCTIONS: &[&str] = &["print", "println", "write", "read"];
const NET_FUNCTIONS: &[&str] = &["fetch", "connect", "listen", "serve"];
const FS_FUNCTIONS: &[&str] = &["readFile", "writeFile", "mkdir", "remove"];
const TIME_FUNCTIONS: &[&str] = &["now", "sleep", "timeout"];

pub struct EffectChecker {
    forbidden: HashSet<Effect>,
}

impl EffectChecker {
    /// Create checker for strict sandbox (no effects allowed).
    pub fn strict() -> Self {
        let mut forbidden = HashSet::new();
        forbidden.insert(Effect::Io);
        forbidden.insert(Effect::Net);
        forbidden.insert(Effect::Fs);
        forbidden.insert(Effect::Time);
        Self { forbidden }
    }

    /// Create checker for relaxed sandbox (only net/fs forbidden).
    pub fn relaxed() -> Self {
        let mut forbidden = HashSet::new();
        forbidden.insert(Effect::Net);
        forbidden.insert(Effect::Fs);
        Self { forbidden }
    }

    /// Create checker that allows all effects.
    pub fn permissive() -> Self {
        Self { forbidden: HashSet::new() }
    }

    /// Check ALM source for effect violations.
    pub fn check(source: &str, mode: &str) -> EffectReport {
        let checker = match mode {
            "strict" => Self::strict(),
            "relaxed" => Self::relaxed(),
            _ => Self::permissive(),
        };

        let prog = match Parser::parse(source) {
            Ok(p) => p,
            Err(_) => {
                return EffectReport {
                    effects: HashSet::new(),
                    violations: vec![],
                };
            }
        };

        let mut effects = HashSet::new();
        let mut violations = Vec::new();

        for stmt in &prog.stmts {
            checker.check_stmt(stmt, &mut effects, &mut violations);
        }

        EffectReport { effects, violations }
    }

    fn check_stmt(&self, stmt: &Stmt, effects: &mut HashSet<Effect>, violations: &mut Vec<EffectViolation>) {
        match stmt {
            Stmt::Bind(b) => self.check_expr(&b.expr, effects, violations),
            Stmt::Annotation(ann) => {
                for arg in &ann.args {
                    self.check_expr(arg, effects, violations);
                }
                if let Some(body) = &ann.body {
                    self.check_expr(body, effects, violations);
                }
            }
            Stmt::Expr(e) => self.check_expr(e, effects, violations),
        }
    }

    fn check_expr(&self, expr: &Expr, effects: &mut HashSet<Effect>, violations: &mut Vec<EffectViolation>) {
        match expr {
            Expr::Effect(inner, span) => {
                // Explicit !effect block
                if let Expr::Ident(name, _) = inner.as_ref() {
                    let effect = match name.as_str() {
                        "io" => Some(Effect::Io),
                        "net" => Some(Effect::Net),
                        "fs" => Some(Effect::Fs),
                        "time" => Some(Effect::Time),
                        _ => None,
                    };
                    if let Some(eff) = effect {
                        effects.insert(eff);
                        if self.forbidden.contains(&eff) {
                            violations.push(EffectViolation {
                                effect: eff,
                                location: format!("{}:{}", span.line, span.col),
                                msg: format!("{eff} not allowed in sandbox"),
                            });
                        }
                    }
                }
                self.check_expr(inner, effects, violations);
            }

            Expr::Call(func_expr, args, span) => {
                // Check if calling a known effectful function
                if let Expr::Ident(name, _) = func_expr.as_ref() {
                    if IO_FUNCTIONS.contains(&name.as_str()) {
                        effects.insert(Effect::Io);
                        if self.forbidden.contains(&Effect::Io) {
                            violations.push(EffectViolation {
                                effect: Effect::Io,
                                location: format!("{}:{}", span.line, span.col),
                                msg: format!("call to '{name}' requires !io"),
                            });
                        }
                    }
                    if NET_FUNCTIONS.contains(&name.as_str()) {
                        effects.insert(Effect::Net);
                        if self.forbidden.contains(&Effect::Net) {
                            violations.push(EffectViolation {
                                effect: Effect::Net,
                                location: format!("{}:{}", span.line, span.col),
                                msg: format!("call to '{name}' requires !net"),
                            });
                        }
                    }
                    if FS_FUNCTIONS.contains(&name.as_str()) {
                        effects.insert(Effect::Fs);
                        if self.forbidden.contains(&Effect::Fs) {
                            violations.push(EffectViolation {
                                effect: Effect::Fs,
                                location: format!("{}:{}", span.line, span.col),
                                msg: format!("call to '{name}' requires !fs"),
                            });
                        }
                    }
                    if TIME_FUNCTIONS.contains(&name.as_str()) {
                        effects.insert(Effect::Time);
                        if self.forbidden.contains(&Effect::Time) {
                            violations.push(EffectViolation {
                                effect: Effect::Time,
                                location: format!("{}:{}", span.line, span.col),
                                msg: format!("call to '{name}' requires !time"),
                            });
                        }
                    }
                }
                self.check_expr(func_expr, effects, violations);
                for a in args { self.check_expr(a, effects, violations); }
            }

            Expr::EnvRef(_, span) => {
                effects.insert(Effect::Io);
                if self.forbidden.contains(&Effect::Io) {
                    violations.push(EffectViolation {
                        effect: Effect::Io,
                        location: format!("{}:{}", span.line, span.col),
                        msg: "$ENV access requires !io".into(),
                    });
                }
            }

            // Recurse into children
            Expr::Block(stmts, _) => {
                for s in stmts { self.check_stmt(s, effects, violations); }
            }
            Expr::Async(inner, _) | Expr::Return(inner, _) | Expr::Metric(inner, _)
            | Expr::Borrow(inner, _, _) | Expr::Move(inner, _) | Expr::Deref(inner, _)
            | Expr::Try(inner, _) | Expr::Increment(inner, _) => {
                self.check_expr(inner, effects, violations);
            }
            Expr::Elvis(a, b, _) => {
                self.check_expr(a, effects, violations);
                self.check_expr(b, effects, violations);
            }
            Expr::Match(scrut, arms, _) => {
                self.check_expr(scrut, effects, violations);
                for arm in arms { self.check_expr(&arm.body, effects, violations); }
            }
            Expr::Loop(cond, body, _) => {
                if let Some(c) = cond { self.check_expr(c, effects, violations); }
                self.check_expr(body, effects, violations);
            }
            Expr::Field(obj, _, _) => self.check_expr(obj, effects, violations),
            Expr::StructLit(_, fields, _) => {
                for (_, v) in fields { self.check_expr(v, effects, violations); }
            }
            Expr::Lambda(_, body, _) => self.check_expr(body, effects, violations),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_code_no_effects() {
        let report = EffectChecker::check("x = 42; x", "strict");
        assert!(report.effects.is_empty());
        assert!(report.violations.is_empty());
    }

    #[test]
    fn test_print_requires_io() {
        let report = EffectChecker::check("print(42)", "strict");
        assert!(report.effects.contains(&Effect::Io));
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].msg.contains("print"));
    }

    #[test]
    fn test_env_ref_requires_io() {
        let report = EffectChecker::check("$HOME", "strict");
        assert!(report.effects.contains(&Effect::Io));
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn test_net_call_forbidden_strict() {
        let report = EffectChecker::check("fetch(url)", "strict");
        assert!(report.effects.contains(&Effect::Net));
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn test_fs_call_forbidden_strict() {
        let report = EffectChecker::check("readFile(path)", "strict");
        assert!(report.effects.contains(&Effect::Fs));
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn test_relaxed_allows_io() {
        let report = EffectChecker::check("print(42)", "relaxed");
        assert!(report.effects.contains(&Effect::Io));
        assert!(report.violations.is_empty()); // IO allowed in relaxed
    }

    #[test]
    fn test_relaxed_blocks_net() {
        let report = EffectChecker::check("fetch(url)", "relaxed");
        assert!(!report.violations.is_empty()); // Net still forbidden
    }

    #[test]
    fn test_permissive_allows_all() {
        let report = EffectChecker::check("print(42);fetch(x);readFile(p)", "permissive");
        assert!(!report.effects.is_empty());
        assert!(report.violations.is_empty()); // All allowed
    }

    #[test]
    fn test_explicit_effect_block() {
        let report = EffectChecker::check("!io", "strict");
        assert!(report.effects.contains(&Effect::Io));
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn test_nested_effect_in_block() {
        let report = EffectChecker::check("{ print(42) }", "strict");
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn test_test_block_with_pure_code() {
        let report = EffectChecker::check("@test(x){assertEq(1,1)}", "strict");
        assert!(report.violations.is_empty()); // assertEq is pure
    }

    #[test]
    fn test_effect_in_annotation() {
        let report = EffectChecker::check("@test(x){print(1)}", "strict");
        assert!(!report.violations.is_empty()); // print in test is still IO
    }
}
