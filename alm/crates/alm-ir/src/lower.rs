//! AST → IR lowering pass.
//!
//! Walks the AST and emits IR instructions for each node.
//! For alpha: wraps everything in a `main` function.

use std::collections::HashMap;

use alm_parser::ast::*;
use alm_parser::Parser;
use crate::*;

pub struct Lowerer {
    module: IrModule,
    /// Current function being built
    func: IrFunction,
    /// Current block for emission
    current_bb: BlockId,
    /// Variable name → (alloca ValId, type)
    locals: Vec<HashMap<String, (ValId, IrType)>>,
}

impl Lowerer {
    pub fn new(module_name: &str) -> Self {
        let func = IrFunction::new("main".into(), vec![], IrType::I64);
        Self {
            module: IrModule::new(module_name.into()),
            func,
            current_bb: BlockId(0),
            locals: vec![HashMap::new()],
        }
    }

    fn emit(&mut self, inst: Inst, ty: IrType) -> ValId {
        self.func.emit(self.current_bb, inst, ty)
    }

    fn current_block_terminated(&self) -> bool {
        let bb = self.func.blocks.iter().find(|b| b.id == self.current_bb);
        if let Some(bb) = bb {
            bb.insts.last().is_some_and(|(_, inst, _)| {
                matches!(inst, Inst::Ret(_) | Inst::Br(_) | Inst::CondBr(_, _, _))
            })
        } else {
            false
        }
    }

    fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.locals.pop();
    }

    fn set_local(&mut self, name: String, ptr: ValId, ty: IrType) {
        self.locals.last_mut().unwrap().insert(name, (ptr, ty));
    }

    fn get_local(&self, name: &str) -> Option<(ValId, IrType)> {
        for scope in self.locals.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    pub fn lower_source(source: &str) -> Result<IrModule, String> {
        let prog = Parser::parse(source).map_err(|e| e.to_string())?;
        let mut lowerer = Lowerer::new("main");
        lowerer.lower_program(&prog)?;
        Ok(lowerer.finish())
    }

    fn lower_program(&mut self, prog: &Program) -> Result<(), String> {
        let mut last_val = None;

        for stmt in &prog.stmts {
            last_val = Some(self.lower_stmt(stmt)?);
        }

        // Return last value or 0, but only if current block isn't already terminated
        if !self.current_block_terminated() {
            let ret_val = last_val.unwrap_or_else(|| self.emit(Inst::ConstInt(0), IrType::I64));
            self.emit(Inst::Ret(ret_val), IrType::Unit);
        }

        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<ValId, String> {
        match stmt {
            Stmt::Bind(bind) => {
                let val = self.lower_expr(&bind.expr)?;
                let ty = self.val_type(&bind.expr);
                let ptr = self.emit(Inst::Alloca(ty.clone()), IrType::Ptr(Box::new(ty.clone())));
                self.emit(Inst::Store(val, ptr), IrType::Unit);
                self.set_local(bind.name.clone(), ptr, ty);
                Ok(val)
            }
            Stmt::Annotation(ann) => {
                // For alpha: annotations are no-ops in compiled code
                // @test blocks don't emit native code (tests use interpreter)
                if let Some(body) = &ann.body {
                    self.lower_expr(body)
                } else {
                    Ok(self.emit(Inst::ConstUnit, IrType::Unit))
                }
            }
            Stmt::Expr(expr) => self.lower_expr(expr),
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<ValId, String> {
        match expr {
            Expr::Literal(lit, _) => self.lower_literal(lit),

            Expr::Ident(name, _) => {
                if let Some((ptr, ty)) = self.get_local(name) {
                    Ok(self.emit(Inst::Load(ptr, ty), IrType::I64))
                } else {
                    Err(format!("undefined variable: {name}"))
                }
            }

            Expr::Block(stmts, _) => {
                self.push_scope();
                let mut last = self.emit(Inst::ConstUnit, IrType::Unit);
                for stmt in stmts {
                    last = self.lower_stmt(stmt)?;
                }
                self.pop_scope();
                Ok(last)
            }

            Expr::Call(func_expr, args, _) => {
                // For alpha: only support calling extern functions by name
                if let Expr::Ident(name, _) = func_expr.as_ref() {
                    let mut arg_vals = Vec::new();
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }
                    Ok(self.emit(
                        Inst::CallExtern(name.clone(), arg_vals, IrType::I64),
                        IrType::I64,
                    ))
                } else {
                    Err("only named function calls supported in alpha".into())
                }
            }

            Expr::Return(inner, _) => {
                let val = self.lower_expr(inner)?;
                self.emit(Inst::Ret(val), IrType::Unit);
                Ok(val)
            }

            Expr::Metric(inner, _) => {
                if let Expr::Ident(name, _) = inner.as_ref() {
                    if !self.module.metrics.contains(name) {
                        self.module.metrics.push(name.clone());
                    }
                    Ok(self.emit(Inst::MetricInc(name.clone()), IrType::Unit))
                } else {
                    Ok(self.emit(Inst::ConstUnit, IrType::Unit))
                }
            }

            Expr::Increment(inner, _) => {
                // #counter++ → metric increment
                if let Expr::Ident(name, _) = inner.as_ref() {
                    if !self.module.metrics.contains(name) {
                        self.module.metrics.push(name.clone());
                    }
                    Ok(self.emit(Inst::MetricInc(name.clone()), IrType::Unit))
                } else {
                    let val = self.lower_expr(inner)?;
                    let one = self.emit(Inst::ConstInt(1), IrType::I64);
                    Ok(self.emit(Inst::IAdd(val, one), IrType::I64))
                }
            }

            Expr::Match(scrutinee, arms, _) => {
                let scrut_val = self.lower_expr(scrutinee)?;

                if arms.is_empty() {
                    return Ok(scrut_val);
                }

                let merge_bb = self.func.fresh_block("match.merge");
                let mut arm_results: Vec<(ValId, BlockId)> = Vec::new();

                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i == arms.len() - 1;
                    let arm_bb = self.func.fresh_block(&format!("match.arm{i}"));
                    let next_bb = if is_last {
                        merge_bb
                    } else {
                        self.func.fresh_block(&format!("match.check{}", i + 1))
                    };

                    // Generate condition check
                    match &arm.pattern {
                        Pattern::Wildcard(_) => {
                            self.emit(Inst::Br(arm_bb), IrType::Unit);
                        }
                        Pattern::Literal(Literal::Int(n), _) => {
                            let cmp_val = self.emit(Inst::ConstInt(*n), IrType::I64);
                            let cond = self.emit(
                                Inst::ICmp(CmpOp::Eq, scrut_val, cmp_val),
                                IrType::Bool,
                            );
                            self.emit(Inst::CondBr(cond, arm_bb, next_bb), IrType::Unit);
                        }
                        Pattern::Error(_) => {
                            // For alpha: always take error arm if we get here
                            self.emit(Inst::Br(arm_bb), IrType::Unit);
                        }
                        Pattern::Ident(_, _) => {
                            // Binding pattern — always matches
                            self.emit(Inst::Br(arm_bb), IrType::Unit);
                        }
                        _ => {
                            self.emit(Inst::Br(arm_bb), IrType::Unit);
                        }
                    }

                    // Emit arm body
                    self.current_bb = arm_bb;
                    let arm_val = self.lower_expr(&arm.body)?;
                    let final_bb = self.current_bb;
                    self.emit(Inst::Br(merge_bb), IrType::Unit);
                    arm_results.push((arm_val, final_bb));

                    if !is_last {
                        self.current_bb = next_bb;
                    }
                }

                self.current_bb = merge_bb;

                // For alpha: just return the last arm's value
                // Full impl would use phi nodes
                if let Some((val, _)) = arm_results.last() {
                    Ok(*val)
                } else {
                    Ok(self.emit(Inst::ConstUnit, IrType::Unit))
                }
            }

            Expr::Loop(count, body, _) => {
                match count {
                    Some(count_expr) => {
                        let n = self.lower_expr(count_expr)?;
                        let counter_ptr = self.emit(
                            Inst::Alloca(IrType::I64),
                            IrType::Ptr(Box::new(IrType::I64)),
                        );
                        let zero = self.emit(Inst::ConstInt(0), IrType::I64);
                        self.emit(Inst::Store(zero, counter_ptr), IrType::Unit);

                        let cond_bb = self.func.fresh_block("loop.cond");
                        let body_bb = self.func.fresh_block("loop.body");
                        let exit_bb = self.func.fresh_block("loop.exit");

                        self.emit(Inst::Br(cond_bb), IrType::Unit);

                        // Condition block
                        self.current_bb = cond_bb;
                        let counter = self.emit(
                            Inst::Load(counter_ptr, IrType::I64),
                            IrType::I64,
                        );
                        let cond = self.emit(
                            Inst::ICmp(CmpOp::Lt, counter, n),
                            IrType::Bool,
                        );
                        self.emit(Inst::CondBr(cond, body_bb, exit_bb), IrType::Unit);

                        // Body block
                        self.current_bb = body_bb;
                        self.lower_expr(body)?;
                        let cur = self.emit(
                            Inst::Load(counter_ptr, IrType::I64),
                            IrType::I64,
                        );
                        let one = self.emit(Inst::ConstInt(1), IrType::I64);
                        let next = self.emit(Inst::IAdd(cur, one), IrType::I64);
                        self.emit(Inst::Store(next, counter_ptr), IrType::Unit);
                        self.emit(Inst::Br(cond_bb), IrType::Unit);

                        self.current_bb = exit_bb;
                        Ok(self.emit(Inst::ConstUnit, IrType::Unit))
                    }
                    None => {
                        // Infinite loop
                        let body_bb = self.func.fresh_block("loop.body");
                        self.emit(Inst::Br(body_bb), IrType::Unit);
                        self.current_bb = body_bb;
                        self.lower_expr(body)?;
                        self.emit(Inst::Br(body_bb), IrType::Unit);
                        // Unreachable after infinite loop, but need a block
                        let exit_bb = self.func.fresh_block("loop.exit");
                        self.current_bb = exit_bb;
                        Ok(self.emit(Inst::ConstUnit, IrType::Unit))
                    }
                }
            }

            Expr::Try(inner, _) => {
                // For alpha: just evaluate inner (no error handling in compiled code yet)
                self.lower_expr(inner)
            }

            Expr::Elvis(inner, default, _) => {
                // For alpha: just evaluate inner
                self.lower_expr(inner).or_else(|_| self.lower_expr(default))
            }

            Expr::Async(inner, _) | Expr::Effect(inner, _) | Expr::Borrow(inner, _, _)
            | Expr::Move(inner, _) | Expr::Deref(inner, _) => {
                // For alpha: these are pass-through
                self.lower_expr(inner)
            }

            Expr::EnvRef(name, _) => {
                // Emit call to getenv
                let name_val = self.emit(Inst::ConstStr(name.clone()), IrType::Str);
                Ok(self.emit(
                    Inst::CallExtern("getenv".into(), vec![name_val], IrType::Str),
                    IrType::Str,
                ))
            }

            Expr::StructLit(_, fields, _) => {
                // For alpha: lower each field value, return last
                let mut last = self.emit(Inst::ConstUnit, IrType::Unit);
                for (_, val) in fields {
                    last = self.lower_expr(val)?;
                }
                Ok(last)
            }

            Expr::Lambda(_, body, _) => {
                // For alpha: lower body inline
                self.lower_expr(body)
            }

            Expr::Field(_, _, _) => {
                // Struct field access not yet supported in compiled code
                Ok(self.emit(Inst::ConstUnit, IrType::Unit))
            }
        }
    }

    fn lower_literal(&mut self, lit: &Literal) -> Result<ValId, String> {
        match lit {
            Literal::Int(v) => Ok(self.emit(Inst::ConstInt(*v), IrType::I64)),
            Literal::Float(v) => Ok(self.emit(Inst::ConstFloat(*v), IrType::F64)),
            Literal::Str(v) => Ok(self.emit(Inst::ConstStr(v.clone()), IrType::Str)),
            Literal::Bool(v) => Ok(self.emit(Inst::ConstBool(*v), IrType::Bool)),
            Literal::Unit => Ok(self.emit(Inst::ConstUnit, IrType::Unit)),
        }
    }

    fn val_type(&self, expr: &Expr) -> IrType {
        match expr {
            Expr::Literal(Literal::Int(_), _) => IrType::I64,
            Expr::Literal(Literal::Float(_), _) => IrType::F64,
            Expr::Literal(Literal::Str(_), _) => IrType::Str,
            Expr::Literal(Literal::Bool(_), _) => IrType::Bool,
            Expr::Literal(Literal::Unit, _) => IrType::Unit,
            _ => IrType::I64, // default for alpha
        }
    }

    fn finish(mut self) -> IrModule {
        self.module.functions.push(self.func);
        self.module
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_simple_binding() {
        let module = Lowerer::lower_source("x = 42; x").unwrap();
        assert_eq!(module.functions.len(), 1);
        let main = &module.functions[0];
        assert_eq!(main.name, "main");
        // Should have: ConstInt(42), Alloca, Store, Load, Ret
        let inst_count: usize = main.blocks.iter().map(|b| b.insts.len()).sum();
        assert!(inst_count >= 4, "expected >=4 instructions, got {inst_count}");
    }

    #[test]
    fn test_lower_metric() {
        let module = Lowerer::lower_source("#rq; #rq").unwrap();
        assert!(module.metrics.contains(&"rq".to_string()));
    }

    #[test]
    fn test_lower_match() {
        let module = Lowerer::lower_source("x = 1; x | 1 => 42 | _ => 0").unwrap();
        let main = &module.functions[0];
        // Should have multiple basic blocks for match
        assert!(main.blocks.len() > 1, "match should create multiple blocks");
    }

    #[test]
    fn test_lower_loop() {
        let module = Lowerer::lower_source("*(3){#iter}").unwrap();
        let main = &module.functions[0];
        // Should have: entry, loop.cond, loop.body, loop.exit
        assert!(main.blocks.len() >= 4, "loop should create 4+ blocks");
    }

    #[test]
    fn test_ir_display() {
        let module = Lowerer::lower_source("x = 42").unwrap();
        let text = format!("{module}");
        assert!(text.contains("fn @main"));
    }
}
