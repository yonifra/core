//! Module Summary (.alm.meta) — compressed semantic summaries for agent context.
//!
//! Instead of reading full source, agents read .alm.meta files to understand
//! module interfaces. ~10x smaller than source.

use alm_parser::ast::*;
use alm_parser::Parser;
use serde::{Deserialize, Serialize};

/// Compressed module summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub path: String,
    pub bindings: Vec<BindingSummary>,
    pub annotations: Vec<AnnotationSummary>,
    pub metrics: Vec<String>,
    pub test_count: u32,
    pub stmt_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingSummary {
    pub name: String,
    pub kind: String, // "int", "str", "struct", "fn", "unknown"
    pub stack: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationSummary {
    pub name: String,
    pub args: Vec<String>,
    pub has_body: bool,
}

impl ModuleSummary {
    /// Generate summary from ALM source.
    pub fn from_source(path: &str, source: &str) -> Result<Self, String> {
        let prog = Parser::parse(source).map_err(|e| e.to_string())?;

        let mut bindings = Vec::new();
        let mut annotations = Vec::new();
        let mut metrics = Vec::new();
        let mut test_count = 0u32;

        for stmt in &prog.stmts {
            match stmt {
                Stmt::Bind(b) => {
                    bindings.push(BindingSummary {
                        name: b.name.clone(),
                        kind: Self::infer_kind(&b.expr),
                        stack: b.stack,
                    });
                }
                Stmt::Annotation(ann) => {
                    if ann.name == "test" {
                        test_count += 1;
                    }
                    let args: Vec<String> = ann.args.iter().map(|a| {
                        match a {
                            Expr::Ident(name, _) => name.clone(),
                            Expr::Literal(Literal::Int(v), _) => v.to_string(),
                            Expr::Literal(Literal::Str(s), _) => format!("\"{s}\""),
                            _ => "?".into(),
                        }
                    }).collect();
                    annotations.push(AnnotationSummary {
                        name: ann.name.clone(),
                        args,
                        has_body: ann.body.is_some(),
                    });
                }
                Stmt::Expr(expr) => {
                    Self::collect_metrics(expr, &mut metrics);
                }
            }
        }

        Ok(ModuleSummary {
            path: path.to_string(),
            bindings,
            annotations,
            metrics,
            test_count,
            stmt_count: prog.stmts.len() as u32,
        })
    }

    fn infer_kind(expr: &Expr) -> String {
        match expr {
            Expr::Literal(Literal::Int(_), _) => "int".into(),
            Expr::Literal(Literal::Float(_), _) => "float".into(),
            Expr::Literal(Literal::Str(_), _) => "str".into(),
            Expr::Literal(Literal::Bool(_), _) => "bool".into(),
            Expr::StructLit(name, _, _) => format!("struct:{name}"),
            Expr::Lambda(_, _, _) => "fn".into(),
            Expr::Block(_, _) => "block".into(),
            Expr::Call(_, _, _) => "call".into(),
            _ => "unknown".into(),
        }
    }

    fn collect_metrics(expr: &Expr, metrics: &mut Vec<String>) {
        match expr {
            Expr::Metric(inner, _) => {
                if let Expr::Ident(name, _) = inner.as_ref() {
                    if !metrics.contains(name) {
                        metrics.push(name.clone());
                    }
                }
            }
            Expr::Increment(inner, _) => {
                if let Expr::Ident(name, _) = inner.as_ref() {
                    if !metrics.contains(name) {
                        metrics.push(name.clone());
                    }
                }
            }
            Expr::Block(stmts, _) => {
                for s in stmts {
                    if let Stmt::Expr(e) = s {
                        Self::collect_metrics(e, metrics);
                    }
                }
            }
            _ => {}
        }
    }

    /// Render as compact text (.alm.meta format).
    pub fn to_meta_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("// .alm.meta for {}\n", self.path));
        out.push_str(&format!("// {} stmts, {} tests\n", self.stmt_count, self.test_count));

        if !self.bindings.is_empty() {
            out.push_str("// bindings:\n");
            for b in &self.bindings {
                let stack = if b.stack { " (stack)" } else { "" };
                out.push_str(&format!("//   {} : {}{}\n", b.name, b.kind, stack));
            }
        }

        if !self.metrics.is_empty() {
            out.push_str(&format!("// metrics: {}\n", self.metrics.join(", ")));
        }

        if !self.annotations.is_empty() {
            out.push_str("// annotations:\n");
            for a in &self.annotations {
                let args = if a.args.is_empty() { String::new() } else { format!("({})", a.args.join(",")) };
                let body = if a.has_body { " {...}" } else { "" };
                out.push_str(&format!("//   @{}{}{}\n", a.name, args, body));
            }
        }

        out
    }

    /// Render as JSON (for programmatic consumption).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_basic() {
        let summary = ModuleSummary::from_source("test.alm", "x = 42; name = \"hello\"").unwrap();
        assert_eq!(summary.bindings.len(), 2);
        assert_eq!(summary.bindings[0].name, "x");
        assert_eq!(summary.bindings[0].kind, "int");
        assert_eq!(summary.bindings[1].kind, "str");
    }

    #[test]
    fn test_summary_tests() {
        let summary = ModuleSummary::from_source("test.alm",
            "@test(a){assertEq(1,1)};@test(b){assert(true)}"
        ).unwrap();
        assert_eq!(summary.test_count, 2);
        assert_eq!(summary.annotations.len(), 2);
    }

    #[test]
    fn test_summary_metrics() {
        let summary = ModuleSummary::from_source("test.alm", "#rq;#rq;#ops").unwrap();
        assert_eq!(summary.metrics, vec!["rq", "ops"]);
    }

    #[test]
    fn test_summary_struct() {
        let summary = ModuleSummary::from_source("test.alm", "p = Point{x:1,y:2}").unwrap();
        assert_eq!(summary.bindings[0].kind, "struct:Point");
    }

    #[test]
    fn test_meta_text() {
        let summary = ModuleSummary::from_source("hello.alm", "x = 42; #rq").unwrap();
        let text = summary.to_meta_text();
        assert!(text.contains(".alm.meta"));
        assert!(text.contains("x : int"));
        assert!(text.contains("rq"));
    }

    #[test]
    fn test_to_json() {
        let summary = ModuleSummary::from_source("test.alm", "x = 1").unwrap();
        let json = summary.to_json();
        assert!(json.contains("\"path\""));
        assert!(json.contains("test.alm"));
    }

    #[test]
    fn test_stack_binding() {
        let summary = ModuleSummary::from_source("test.alm", "x := 42").unwrap();
        assert!(summary.bindings[0].stack);
    }
}
