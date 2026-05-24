//! ALM LLVM Backend — lowers ALM-IR to LLVM IR and emits native object files.
//!
//! Uses inkwell (safe LLVM wrapper) to generate LLVM IR from ALM-IR,
//! then compiles to native object code for the host platform.

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::OptimizationLevel;
use inkwell::{AddressSpace, IntPredicate};

use alm_ir::*;
use alm_ir::lower::Lowerer;

use std::collections::HashMap;
use std::path::Path;

pub struct Codegen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        Self { context, module, builder }
    }

    /// Compile ALM source to LLVM IR string.
    pub fn compile_to_ir(source: &str) -> Result<String, String> {
        let ir_module = Lowerer::lower_source(source)?;
        let context = Context::create();
        let mut codegen = Codegen::new(&context, &ir_module.name);
        codegen.emit_module(&ir_module)?;
        Ok(codegen.module.print_to_string().to_string())
    }

    /// Compile ALM source to native object file.
    pub fn compile_to_object(source: &str, output_path: &str) -> Result<(), String> {
        let ir_module = Lowerer::lower_source(source)?;
        let context = Context::create();
        let mut codegen = Codegen::new(&context, &ir_module.name);
        codegen.emit_module(&ir_module)?;
        codegen.write_object(output_path)
    }

    /// Compile ALM source to native executable.
    pub fn compile_to_executable(source: &str, output_path: &str) -> Result<(), String> {
        let obj_path = format!("{output_path}.o");
        Self::compile_to_object(source, &obj_path)?;
        Self::link_executable(&obj_path, output_path)?;
        // Clean up object file
        let _ = std::fs::remove_file(&obj_path);
        Ok(())
    }

    fn emit_module(&mut self, ir_module: &IrModule) -> Result<(), String> {
        // Emit metric globals
        for metric in &ir_module.metrics {
            let i64_type = self.context.i64_type();
            let global = self.module.add_global(i64_type, None, &format!("metric.{metric}"));
            global.set_initializer(&i64_type.const_int(0, false));
        }

        // Emit string globals
        // (handled inline during function emission)

        // Emit functions
        for func in &ir_module.functions {
            self.emit_function(func, &ir_module.metrics)?;
        }

        // Verify module
        self.module.verify().map_err(|e| format!("LLVM verify failed: {}", e.to_string()))?;

        Ok(())
    }

    fn emit_function(&mut self, func: &IrFunction, metrics: &[String]) -> Result<(), String> {
        let i64_type = self.context.i64_type();

        // Build function type
        let param_types: Vec<BasicMetadataTypeEnum> = func.params.iter().map(|(_, ty)| {
            self.ir_type_to_llvm(ty).into()
        }).collect();

        let fn_type = if func.ret_type == IrType::Unit {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            i64_type.fn_type(&param_types, false)
        };

        let function = self.module.add_function(&func.name, fn_type, None);

        // Create basic blocks
        let mut bb_map: HashMap<BlockId, inkwell::basic_block::BasicBlock> = HashMap::new();
        for bb in &func.blocks {
            let llvm_bb = self.context.append_basic_block(function, &bb.name);
            bb_map.insert(bb.id, llvm_bb);
        }

        // Value map: ValId → LLVM value
        let mut val_map: HashMap<ValId, BasicValueEnum<'ctx>> = HashMap::new();
        let mut string_counter = 0u32;

        // Emit instructions for each block
        for bb in &func.blocks {
            let llvm_bb = bb_map[&bb.id];
            self.builder.position_at_end(llvm_bb);

            for (val_id, inst, _ty) in &bb.insts {
                let llvm_val: Option<BasicValueEnum<'ctx>> = match inst {
                    Inst::ConstInt(v) => {
                        Some(i64_type.const_int(*v as u64, true).into())
                    }
                    Inst::ConstFloat(v) => {
                        Some(self.context.f64_type().const_float(*v).into())
                    }
                    Inst::ConstBool(v) => {
                        Some(self.context.bool_type().const_int(*v as u64, false).into())
                    }
                    Inst::ConstStr(s) => {
                        let global_name = format!(".str.{string_counter}");
                        string_counter += 1;
                        let str_val = self.context.const_string(s.as_bytes(), true);
                        let global = self.module.add_global(str_val.get_type(), None, &global_name);
                        global.set_initializer(&str_val);
                        global.set_constant(true);
                        Some(global.as_pointer_value().into())
                    }
                    Inst::ConstUnit => None, // void has no value

                    Inst::Alloca(ty) => {
                        let llvm_ty = self.ir_type_to_llvm(ty);
                        let ptr = self.builder.build_alloca(llvm_ty, "alloca")
                            .map_err(|e| e.to_string())?;
                        Some(ptr.into())
                    }

                    Inst::Store(val, ptr) => {
                        if let (Some(v), Some(p)) = (val_map.get(val), val_map.get(ptr)) {
                            let ptr_val = p.into_pointer_value();
                            self.builder.build_store(ptr_val, *v)
                                .map_err(|e| e.to_string())?;
                        }
                        None
                    }

                    Inst::Load(ptr, _ty) => {
                        if let Some(p) = val_map.get(ptr) {
                            let ptr_val = p.into_pointer_value();
                            let loaded = self.builder.build_load(i64_type, ptr_val, "load")
                                .map_err(|e| e.to_string())?;
                            Some(loaded)
                        } else {
                            Some(i64_type.const_int(0, false).into())
                        }
                    }

                    Inst::Ret(val) => {
                        if let Some(v) = val_map.get(val) {
                            self.builder.build_return(Some(v))
                                .map_err(|e| e.to_string())?;
                        } else {
                            self.builder.build_return(Some(&i64_type.const_int(0, false)))
                                .map_err(|e| e.to_string())?;
                        }
                        None
                    }

                    Inst::CondBr(cond, then_bb, else_bb) => {
                        if let Some(c) = val_map.get(cond) {
                            let cond_int = c.into_int_value();
                            self.builder.build_conditional_branch(
                                cond_int,
                                bb_map[then_bb],
                                bb_map[else_bb],
                            ).map_err(|e| e.to_string())?;
                        }
                        None
                    }

                    Inst::Br(target) => {
                        self.builder.build_unconditional_branch(bb_map[target])
                            .map_err(|e| e.to_string())?;
                        None
                    }

                    Inst::ICmp(op, lhs, rhs) => {
                        if let (Some(l), Some(r)) = (val_map.get(lhs), val_map.get(rhs)) {
                            let pred = match op {
                                CmpOp::Eq => IntPredicate::EQ,
                                CmpOp::Ne => IntPredicate::NE,
                                CmpOp::Lt => IntPredicate::SLT,
                                CmpOp::Le => IntPredicate::SLE,
                                CmpOp::Gt => IntPredicate::SGT,
                                CmpOp::Ge => IntPredicate::SGE,
                            };
                            let result = self.builder.build_int_compare(
                                pred,
                                l.into_int_value(),
                                r.into_int_value(),
                                "cmp",
                            ).map_err(|e| e.to_string())?;
                            Some(result.into())
                        } else {
                            Some(self.context.bool_type().const_int(0, false).into())
                        }
                    }

                    Inst::IAdd(lhs, rhs) => {
                        if let (Some(l), Some(r)) = (val_map.get(lhs), val_map.get(rhs)) {
                            let result = self.builder.build_int_add(
                                l.into_int_value(),
                                r.into_int_value(),
                                "add",
                            ).map_err(|e| e.to_string())?;
                            Some(result.into())
                        } else {
                            Some(i64_type.const_int(0, false).into())
                        }
                    }

                    Inst::ISub(lhs, rhs) => {
                        if let (Some(l), Some(r)) = (val_map.get(lhs), val_map.get(rhs)) {
                            let result = self.builder.build_int_sub(
                                l.into_int_value(),
                                r.into_int_value(),
                                "sub",
                            ).map_err(|e| e.to_string())?;
                            Some(result.into())
                        } else {
                            Some(i64_type.const_int(0, false).into())
                        }
                    }

                    Inst::IMul(lhs, rhs) => {
                        if let (Some(l), Some(r)) = (val_map.get(lhs), val_map.get(rhs)) {
                            let result = self.builder.build_int_mul(
                                l.into_int_value(),
                                r.into_int_value(),
                                "mul",
                            ).map_err(|e| e.to_string())?;
                            Some(result.into())
                        } else {
                            Some(i64_type.const_int(0, false).into())
                        }
                    }

                    Inst::MetricInc(name) => {
                        let global_name = format!("metric.{name}");
                        if let Some(global) = self.module.get_global(&global_name) {
                            let ptr = global.as_pointer_value();
                            let cur = self.builder.build_load(i64_type, ptr, "metric.cur")
                                .map_err(|e| e.to_string())?;
                            let one = i64_type.const_int(1, false);
                            let inc = self.builder.build_int_add(
                                cur.into_int_value(),
                                one,
                                "metric.inc",
                            ).map_err(|e| e.to_string())?;
                            self.builder.build_store(ptr, inc)
                                .map_err(|e| e.to_string())?;
                        }
                        None
                    }

                    Inst::CallExtern(name, args, ret_ty) => {
                        // Declare extern if not exists
                        let callee = self.get_or_declare_extern(name, args.len(), ret_ty);
                        let arg_vals: Vec<BasicMetadataValueEnum> = args.iter()
                            .filter_map(|a| val_map.get(a).map(|v| (*v).into()))
                            .collect();
                        let call = self.builder.build_call(callee, &arg_vals, "call")
                            .map_err(|e| e.to_string())?;
                        call.try_as_basic_value().left()
                    }

                    Inst::Call(func_val, args) => {
                        // For alpha: skip indirect calls
                        None
                    }

                    Inst::Param(idx) => {
                        Some(function.get_nth_param(*idx).unwrap())
                    }

                    Inst::Phi(_, _) => {
                        // PHI nodes need special handling — skip in alpha
                        None
                    }

                    Inst::Nop => None,
                };

                if let Some(v) = llvm_val {
                    val_map.insert(*val_id, v);
                }
            }
        }

        Ok(())
    }

    fn get_or_declare_extern(
        &self,
        name: &str,
        arg_count: usize,
        ret_ty: &IrType,
    ) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function(name) {
            return f;
        }

        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());

        // Common C functions
        let (param_types, ret): (Vec<BasicMetadataTypeEnum>, _) = match name {
            "printf" => {
                let params: Vec<BasicMetadataTypeEnum> = vec![ptr_type.into()];
                (params, None) // variadic, returns i32 but we ignore
            }
            "getenv" => {
                (vec![ptr_type.into()], Some(ptr_type.as_basic_type_enum()))
            }
            "exit" => {
                (vec![i64_type.into()], None)
            }
            _ => {
                // Generic: all i64 params, i64 return
                let params: Vec<BasicMetadataTypeEnum> =
                    (0..arg_count).map(|_| i64_type.into()).collect();
                (params, Some(i64_type.as_basic_type_enum()))
            }
        };

        let fn_type = match ret {
            Some(BasicTypeEnum::IntType(t)) => t.fn_type(&param_types, name == "printf"),
            Some(BasicTypeEnum::PointerType(t)) => t.fn_type(&param_types, name == "printf"),
            None => self.context.i32_type().fn_type(&param_types, name == "printf"),
            _ => i64_type.fn_type(&param_types, false),
        };

        self.module.add_function(name, fn_type, None)
    }

    fn ir_type_to_llvm(&self, ty: &IrType) -> BasicTypeEnum<'ctx> {
        match ty {
            IrType::I64 => self.context.i64_type().into(),
            IrType::F64 => self.context.f64_type().into(),
            IrType::Bool => self.context.bool_type().into(),
            IrType::Str => self.context.ptr_type(AddressSpace::default()).into(),
            IrType::Unit => self.context.i64_type().into(), // represent void as i64(0)
            IrType::Ptr(_) => self.context.ptr_type(AddressSpace::default()).into(),
            IrType::Fn(_, _) => self.context.ptr_type(AddressSpace::default()).into(),
        }
    }

    fn write_object(&self, output_path: &str) -> Result<(), String> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| format!("failed to init native target: {e}"))?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("failed to get target: {}", e.to_string()))?;

        let cpu = TargetMachine::get_host_cpu_name();
        let features = TargetMachine::get_host_cpu_features();

        let machine = target
            .create_target_machine(
                &triple,
                cpu.to_str().unwrap_or("generic"),
                features.to_str().unwrap_or(""),
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("failed to create target machine")?;

        machine
            .write_to_file(&self.module, FileType::Object, Path::new(output_path))
            .map_err(|e| format!("failed to write object: {}", e.to_string()))?;

        Ok(())
    }

    fn link_executable(obj_path: &str, output_path: &str) -> Result<(), String> {
        // Use system cc to link
        let status = std::process::Command::new("cc")
            .args([obj_path, "-o", output_path])
            .status()
            .map_err(|e| format!("failed to run linker: {e}"))?;

        if !status.success() {
            return Err(format!("linker failed with status: {status}"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_to_ir() {
        let ir = Codegen::compile_to_ir("x = 42; x").unwrap();
        assert!(ir.contains("define"), "should contain function definition");
        assert!(ir.contains("42"), "should contain constant 42");
        assert!(ir.contains("ret"), "should contain return");
    }

    #[test]
    fn test_compile_return_literal() {
        let ir = Codegen::compile_to_ir(">99").unwrap();
        assert!(ir.contains("99"));
        assert!(ir.contains("ret"));
    }

    #[test]
    fn test_compile_metric() {
        let ir = Codegen::compile_to_ir("#rq; #rq").unwrap();
        assert!(ir.contains("metric.rq"), "should have metric global");
    }

    #[test]
    fn test_compile_to_object() {
        let tmp = std::env::temp_dir().join("alm_test.o");
        let result = Codegen::compile_to_object(
            "x = 42; x",
            tmp.to_str().unwrap(),
        );
        assert!(result.is_ok(), "object compilation failed: {result:?}");
        assert!(tmp.exists(), "object file should exist");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_compile_to_executable() {
        let tmp_dir = std::env::temp_dir();
        let exe_path = tmp_dir.join("alm_test_exe");
        let result = Codegen::compile_to_executable(
            ">42",
            exe_path.to_str().unwrap(),
        );
        assert!(result.is_ok(), "executable compilation failed: {result:?}");
        assert!(exe_path.exists(), "executable should exist");

        // Run it and check exit code
        let output = std::process::Command::new(exe_path.to_str().unwrap())
            .output()
            .expect("failed to run compiled binary");
        // main returns 42, which becomes exit code 42
        assert_eq!(output.status.code(), Some(42));

        let _ = std::fs::remove_file(&exe_path);
    }

    #[test]
    fn test_compile_match() {
        let ir = Codegen::compile_to_ir("x = 1; x | 1 => 42 | _ => 0").unwrap();
        assert!(ir.contains("match.arm"), "should have match arm blocks");
    }

    #[test]
    fn test_compile_loop() {
        let ir = Codegen::compile_to_ir("*(3){#iter}").unwrap();
        assert!(ir.contains("loop.cond"), "should have loop condition block");
        assert!(ir.contains("loop.body"), "should have loop body block");
    }
}
