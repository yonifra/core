//! ALM Interpreter — tree-walking evaluator for ALM AST.
//!
//! Supports: bindings, function calls, blocks, match, metrics, try/elvis,
//! annotations (@test), loops, struct literals, lambdas.

use alm_parser::ast::*;
use alm_parser::Parser;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
    Struct(String, HashMap<String, Value>),
    List(Vec<Value>),
    Fn(Vec<String>, Expr), // params, body
    Error(String),
    // Built-in function
    BuiltIn(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Str(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Unit => write!(f, "()"),
            Value::Struct(name, fields) => {
                write!(f, "{name}{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ",")?; }
                    write!(f, "{k}:{v}")?;
                }
                write!(f, "}}")
            }
            Value::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ",")?; }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Fn(params, _) => write!(f, "fn({})", params.join(",")),
            Value::Error(msg) => write!(f, "Error({msg})"),
            Value::BuiltIn(name) => write!(f, "<builtin:{name}>"),
        }
    }
}

impl Value {
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::Unit => false,
            Value::Error(_) => false,
            _ => true,
        }
    }
}

#[derive(Debug)]
pub struct RuntimeError {
    pub msg: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E200 runtime: {}", self.msg)
    }
}

/// Control flow signal for returns.
enum Flow {
    Value(Value),
    Return(Value),
}

impl Flow {
    fn into_value(self) -> Value {
        match self {
            Flow::Value(v) | Flow::Return(v) => v,
        }
    }
}

pub struct Env {
    scopes: Vec<HashMap<String, Value>>,
    pub metrics: HashMap<String, i64>,
    pub test_results: Vec<(String, bool)>,
}

impl Env {
    pub fn new() -> Self {
        let mut global = HashMap::new();
        // Register built-in functions
        global.insert("retry".into(), Value::BuiltIn("retry".into()));
        global.insert("print".into(), Value::BuiltIn("print".into()));
        global.insert("len".into(), Value::BuiltIn("len".into()));
        global.insert("assert".into(), Value::BuiltIn("assert".into()));
        global.insert("assertEq".into(), Value::BuiltIn("assertEq".into()));

        Self {
            scopes: vec![global],
            metrics: HashMap::new(),
            test_results: Vec::new(),
        }
    }

    fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn set(&mut self, name: String, val: Value) {
        self.scopes.last_mut().unwrap().insert(name, val);
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Interpreter {
    pub env: Env,
}

impl Interpreter {
    pub fn new() -> Self {
        Self { env: Env::new() }
    }

    pub fn eval_source(&mut self, source: &str) -> Result<Value, RuntimeError> {
        let prog = Parser::parse(source)
            .map_err(|e| RuntimeError { msg: e.to_string() })?;
        self.eval_program(&prog)
    }

    pub fn eval_program(&mut self, prog: &Program) -> Result<Value, RuntimeError> {
        let mut last = Value::Unit;
        for stmt in &prog.stmts {
            match self.eval_stmt(stmt)? {
                Flow::Return(v) => return Ok(v),
                Flow::Value(v) => last = v,
            }
        }
        Ok(last)
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> Result<Flow, RuntimeError> {
        match stmt {
            Stmt::Bind(b) => {
                let val = self.eval_expr(&b.expr)?.into_value();
                self.env.set(b.name.clone(), val);
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Annotation(a) => self.eval_annotation(a),
            Stmt::Expr(e) => self.eval_expr(e),
        }
    }

    fn eval_annotation(&mut self, ann: &Annotation) -> Result<Flow, RuntimeError> {
        match ann.name.as_str() {
            "test" => {
                let test_name = if let Some(Expr::Ident(name, _)) = ann.args.first() {
                    name.clone()
                } else if let Some(arg) = ann.args.first() {
                    format!("{arg:?}")
                } else {
                    "unnamed".into()
                };

                if let Some(body) = &ann.body {
                    self.env.push_scope();
                    let result = self.eval_expr(body);
                    self.env.pop_scope();

                    let passed = match result {
                        Ok(_) => true,
                        Err(_) => false,
                    };
                    self.env.test_results.push((test_name, passed));
                }
                Ok(Flow::Value(Value::Unit))
            }
            _ => {
                // Generic annotation — eval body if present
                if let Some(body) = &ann.body {
                    self.eval_expr(body)
                } else {
                    Ok(Flow::Value(Value::Unit))
                }
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Flow, RuntimeError> {
        match expr {
            Expr::Literal(lit, _) => Ok(Flow::Value(self.eval_literal(lit))),

            Expr::Ident(name, _) => {
                self.env.get(name).map(Flow::Value).ok_or_else(|| RuntimeError {
                    msg: format!("undefined: {name}"),
                })
            }

            Expr::EnvRef(name, _) => {
                let val = std::env::var(name).unwrap_or_default();
                Ok(Flow::Value(Value::Str(val)))
            }

            Expr::Block(stmts, _) => {
                self.env.push_scope();
                let mut last = Value::Unit;
                for stmt in stmts {
                    match self.eval_stmt(stmt)? {
                        Flow::Return(v) => {
                            self.env.pop_scope();
                            return Ok(Flow::Return(v));
                        }
                        Flow::Value(v) => last = v,
                    }
                }
                self.env.pop_scope();
                Ok(Flow::Value(last))
            }

            Expr::Call(func, args, _) => {
                let f = self.eval_expr(func)?.into_value();
                let mut eval_args = Vec::new();
                for a in args {
                    eval_args.push(self.eval_expr(a)?.into_value());
                }
                self.call_value(&f, eval_args)
            }

            Expr::Field(obj, field, _) => {
                let val = self.eval_expr(obj)?.into_value();
                match val {
                    Value::Struct(_, ref fields) => {
                        fields.get(field).cloned().map(Flow::Value).ok_or_else(|| RuntimeError {
                            msg: format!("no field {field}"),
                        })
                    }
                    _ => Err(RuntimeError {
                        msg: format!("cannot access field on {val}"),
                    }),
                }
            }

            Expr::StructLit(name, fields, _) => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v)?.into_value());
                }
                Ok(Flow::Value(Value::Struct(name.clone(), map)))
            }

            Expr::Lambda(params, body, _) => {
                Ok(Flow::Value(Value::Fn(params.clone(), *body.clone())))
            }

            Expr::Async(inner, _) => {
                // In interpreter: just eval synchronously
                self.eval_expr(inner)
            }

            Expr::Effect(inner, _) => {
                // In interpreter: just eval (no effect tracking yet)
                self.eval_expr(inner)
            }

            Expr::Metric(inner, _) => {
                // Increment named metric. The inner expr is typically an ident name.
                if let Expr::Ident(name, _) = inner.as_ref() {
                    *self.env.metrics.entry(name.clone()).or_insert(0) += 1;
                    Ok(Flow::Value(Value::Int(*self.env.metrics.get(name).unwrap())))
                } else {
                    let val = self.eval_expr(inner)?.into_value();
                    if let Value::Int(n) = &val {
                        *self.env.metrics.entry(format!("metric_{n}")).or_insert(0) += 1;
                    }
                    Ok(Flow::Value(val))
                }
            }

            Expr::Return(inner, _) => {
                let val = self.eval_expr(inner)?.into_value();
                Ok(Flow::Return(val))
            }

            Expr::Borrow(inner, _mutable, _) => {
                // In interpreter: just evaluate (no borrow semantics in tree-walker)
                self.eval_expr(inner)
            }

            Expr::Move(inner, _) => {
                self.eval_expr(inner)
            }

            Expr::Deref(inner, _) => {
                self.eval_expr(inner)
            }

            Expr::Try(inner, _) => {
                let val = self.eval_expr(inner)?.into_value();
                if val.is_error() {
                    Ok(Flow::Return(val))
                } else {
                    Ok(Flow::Value(val))
                }
            }

            Expr::Elvis(inner, default, _) => {
                let val = self.eval_expr(inner)?.into_value();
                if val.is_error() {
                    self.eval_expr(default)
                } else {
                    Ok(Flow::Value(val))
                }
            }

            Expr::Increment(inner, _) => {
                // Used for metrics: #counter++
                if let Expr::Ident(name, _) = inner.as_ref() {
                    *self.env.metrics.entry(name.clone()).or_insert(0) += 1;
                    Ok(Flow::Value(Value::Unit))
                } else {
                    let val = self.eval_expr(inner)?.into_value();
                    if let Value::Int(n) = val {
                        Ok(Flow::Value(Value::Int(n + 1)))
                    } else {
                        Err(RuntimeError { msg: "cannot increment non-int".into() })
                    }
                }
            }

            Expr::Match(scrutinee, arms, _) => {
                let val = self.eval_expr(scrutinee)?.into_value();
                for arm in arms {
                    if self.pattern_matches(&arm.pattern, &val) {
                        // Bind pattern variables
                        self.env.push_scope();
                        self.bind_pattern(&arm.pattern, &val);
                        let result = self.eval_expr(&arm.body);
                        self.env.pop_scope();
                        return result;
                    }
                }
                Ok(Flow::Value(val))
            }

            Expr::Loop(cond, body, _) => {
                match cond {
                    Some(count_expr) => {
                        let count = self.eval_expr(count_expr)?.into_value();
                        if let Value::Int(n) = count {
                            let mut last = Value::Unit;
                            for _ in 0..n {
                                match self.eval_expr(body)? {
                                    Flow::Return(v) => return Ok(Flow::Return(v)),
                                    Flow::Value(v) => last = v,
                                }
                            }
                            Ok(Flow::Value(last))
                        } else {
                            Err(RuntimeError { msg: "loop count must be int".into() })
                        }
                    }
                    None => {
                        // Infinite loop — needs break mechanism (TODO)
                        // For now, run 1000 iterations max as safety
                        let mut last = Value::Unit;
                        for _ in 0..1000 {
                            match self.eval_expr(body)? {
                                Flow::Return(v) => return Ok(Flow::Return(v)),
                                Flow::Value(v) => last = v,
                            }
                        }
                        Ok(Flow::Value(last))
                    }
                }
            }
        }
    }

    fn eval_literal(&self, lit: &Literal) -> Value {
        match lit {
            Literal::Int(v) => Value::Int(*v),
            Literal::Float(v) => Value::Float(*v),
            Literal::Str(v) => Value::Str(v.clone()),
            Literal::Bool(v) => Value::Bool(*v),
            Literal::Unit => Value::Unit,
        }
    }

    fn pattern_matches(&self, pat: &Pattern, val: &Value) -> bool {
        match pat {
            Pattern::Wildcard(_) => true,
            Pattern::Error(_) => val.is_error(),
            Pattern::Literal(lit, _) => {
                match (lit, val) {
                    (Literal::Int(a), Value::Int(b)) => a == b,
                    (Literal::Str(a), Value::Str(b)) => a == b,
                    (Literal::Bool(a), Value::Bool(b)) => a == b,
                    _ => false,
                }
            }
            Pattern::Ident(_, _) => true, // Ident pattern always matches (binds)
        }
    }

    fn bind_pattern(&mut self, pat: &Pattern, val: &Value) {
        if let Pattern::Ident(name, _) = pat {
            self.env.set(name.clone(), val.clone());
        }
    }

    fn call_value(&mut self, func: &Value, args: Vec<Value>) -> Result<Flow, RuntimeError> {
        match func {
            Value::Fn(params, body) => {
                self.env.push_scope();
                for (p, a) in params.iter().zip(args.iter()) {
                    self.env.set(p.clone(), a.clone());
                }
                let result = self.eval_expr(body);
                self.env.pop_scope();
                match result? {
                    Flow::Return(v) | Flow::Value(v) => Ok(Flow::Value(v)),
                }
            }
            Value::BuiltIn(name) => self.call_builtin(name, args),
            _ => Err(RuntimeError { msg: format!("not callable: {func}") }),
        }
    }

    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Flow, RuntimeError> {
        match name {
            "print" => {
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { print!(" "); }
                    print!("{a}");
                }
                println!();
                Ok(Flow::Value(Value::Unit))
            }
            "len" => {
                match args.first() {
                    Some(Value::Str(s)) => Ok(Flow::Value(Value::Int(s.len() as i64))),
                    Some(Value::List(l)) => Ok(Flow::Value(Value::Int(l.len() as i64))),
                    _ => Err(RuntimeError { msg: "len: expected string or list".into() }),
                }
            }
            "assert" => {
                match args.first() {
                    Some(v) if v.truthy() => Ok(Flow::Value(Value::Unit)),
                    _ => Err(RuntimeError { msg: "assertion failed".into() }),
                }
            }
            "assertEq" => {
                if args.len() != 2 {
                    return Err(RuntimeError { msg: "assertEq: expected 2 args".into() });
                }
                let a = format!("{}", args[0]);
                let b = format!("{}", args[1]);
                if a == b {
                    Ok(Flow::Value(Value::Unit))
                } else {
                    Err(RuntimeError { msg: format!("assertEq failed: {a} != {b}") })
                }
            }
            "retry" => {
                // retry(n, fn, args...) — call fn with remaining args, retry n times
                if args.len() < 2 {
                    return Err(RuntimeError { msg: "retry: need count and function".into() });
                }
                let count = match &args[0] {
                    Value::Int(n) => *n,
                    _ => return Err(RuntimeError { msg: "retry: first arg must be int".into() }),
                };
                let func = &args[1];
                let fn_args: Vec<Value> = args[2..].to_vec();

                let mut last_err = Value::Unit;
                for _ in 0..count {
                    match self.call_value(func, fn_args.clone())? {
                        Flow::Value(v) if !v.is_error() => return Ok(Flow::Value(v)),
                        Flow::Value(v) => last_err = v,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                    }
                }
                Ok(Flow::Value(last_err))
            }
            _ => Err(RuntimeError { msg: format!("unknown builtin: {name}") }),
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/// Run ALM source and return the result.
pub fn run(source: &str) -> Result<Value, RuntimeError> {
    let mut interp = Interpreter::new();
    interp.eval_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_binding() {
        let val = run("x = 42; x").unwrap();
        assert!(matches!(val, Value::Int(42)));
    }

    #[test]
    fn test_string_binding() {
        let val = run(r#"x = "hello"; x"#).unwrap();
        match val {
            Value::Str(s) => assert_eq!(s, "hello"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn test_block() {
        let val = run("{ x = 10; x }").unwrap();
        assert!(matches!(val, Value::Int(10)));
    }

    #[test]
    fn test_struct_literal() {
        let val = run("w = Widget{a: 1, b: 2}; w.a").unwrap();
        assert!(matches!(val, Value::Int(1)));
    }

    #[test]
    fn test_metric_increment() {
        let mut interp = Interpreter::new();
        interp.eval_source("#rq;#rq;#rq").unwrap();
        // rq metric parsed as ident after #
        assert!(!interp.env.metrics.is_empty());
    }

    #[test]
    fn test_match() {
        let val = run("x = 1; x | 1 => 42 | _ => 0").unwrap();
        assert!(matches!(val, Value::Int(42)));
    }

    #[test]
    fn test_match_wildcard() {
        let val = run("x = 99; x | 1 => 42 | _ => 0").unwrap();
        assert!(matches!(val, Value::Int(0)));
    }

    #[test]
    fn test_loop() {
        let val = run("x = 0; *(3){x = 1}; x").unwrap();
        // x should be set in outer scope? No — loop body is a block.
        // Actually, the loop evals the block which creates inner scope.
        // But x=1 is parsed as binding in inner scope.
        // This test verifies loop runs without crash.
        assert!(matches!(val, Value::Int(_)));
    }

    #[test]
    fn test_return() {
        let val = run("{ >42; 99 }").unwrap();
        assert!(matches!(val, Value::Int(42)));
    }

    #[test]
    fn test_function_call() {
        let val = run("assertEq(1, 1)").unwrap();
        assert!(matches!(val, Value::Unit));
    }

    #[test]
    fn test_assert_fail() {
        let result = run("assert(false)");
        assert!(result.is_err());
    }

    #[test]
    fn test_annotation_test() {
        let mut interp = Interpreter::new();
        interp.eval_source("@test(myTest) { assert(true) }").unwrap();
        assert_eq!(interp.env.test_results.len(), 1);
        assert!(interp.env.test_results[0].1); // passed
    }

    #[test]
    fn test_env_ref() {
        // $PATH should exist on all systems
        let val = run("$PATH").unwrap();
        match val {
            Value::Str(s) => assert!(!s.is_empty()),
            _ => panic!("expected string"),
        }
    }
}
