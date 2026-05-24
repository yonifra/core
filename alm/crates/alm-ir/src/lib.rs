//! ALM-IR — SSA-form intermediate representation.
//!
//! Bridges AST → LLVM. Flat basic-block structure with typed values.
//! Each instruction produces a named SSA value (%0, %1, ...).
//!
//! Design goals:
//! - Direct 1:1 mapping to LLVM IR instructions
//! - Explicit types (no inference needed at this level)
//! - Simple enough for alpha — no phi nodes yet (structured control flow)

pub mod lower;

use std::fmt;

/// Type system at IR level — much simpler than source-level.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrType {
    I64,
    F64,
    Bool,
    Str,    // ptr to null-terminated string
    Unit,   // void
    Ptr(Box<IrType>), // pointer to T
    Fn(Vec<IrType>, Box<IrType>), // (params) -> ret
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrType::I64 => write!(f, "i64"),
            IrType::F64 => write!(f, "f64"),
            IrType::Bool => write!(f, "i1"),
            IrType::Str => write!(f, "ptr"),
            IrType::Unit => write!(f, "void"),
            IrType::Ptr(t) => write!(f, "ptr<{t}>"),
            IrType::Fn(params, ret) => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
        }
    }
}

/// SSA value reference — index into function's value table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValId(pub u32);

impl fmt::Display for ValId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%{}", self.0)
    }
}

/// A single IR instruction that produces a value.
#[derive(Debug, Clone)]
pub enum Inst {
    /// Integer constant
    ConstInt(i64),
    /// Float constant
    ConstFloat(f64),
    /// Boolean constant
    ConstBool(bool),
    /// String constant (global)
    ConstStr(String),
    /// Unit value
    ConstUnit,

    /// Allocate local variable on stack
    Alloca(IrType),
    /// Store value to pointer
    Store(ValId, ValId), // store val, ptr
    /// Load from pointer
    Load(ValId, IrType), // load ptr : type

    /// Call a function: call func(args...)
    Call(ValId, Vec<ValId>),
    /// Call external/builtin function by name
    CallExtern(String, Vec<ValId>, IrType),

    /// Return value from function
    Ret(ValId),

    /// Conditional branch
    CondBr(ValId, BlockId, BlockId), // cond, then_bb, else_bb
    /// Unconditional branch
    Br(BlockId),

    /// Integer comparison
    ICmp(CmpOp, ValId, ValId),
    /// Integer arithmetic
    IAdd(ValId, ValId),
    ISub(ValId, ValId),
    IMul(ValId, ValId),

    /// Phi node (for SSA joins)
    Phi(IrType, Vec<(ValId, BlockId)>),

    /// Metric increment: atomic add to global counter
    MetricInc(String),

    /// Get function parameter by index
    Param(u32),

    /// No-op placeholder
    Nop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmpOp::Eq => write!(f, "eq"),
            CmpOp::Ne => write!(f, "ne"),
            CmpOp::Lt => write!(f, "lt"),
            CmpOp::Le => write!(f, "le"),
            CmpOp::Gt => write!(f, "gt"),
            CmpOp::Ge => write!(f, "ge"),
        }
    }
}

/// Basic block ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// A basic block — sequence of instructions with a single entry.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub name: String,
    pub insts: Vec<(ValId, Inst, IrType)>, // (result_val, instruction, result_type)
}

/// A function in the IR.
#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub ret_type: IrType,
    pub blocks: Vec<BasicBlock>,
    pub next_val: u32,
    pub next_block: u32,
}

impl IrFunction {
    pub fn new(name: String, params: Vec<(String, IrType)>, ret_type: IrType) -> Self {
        let entry = BasicBlock {
            id: BlockId(0),
            name: "entry".into(),
            insts: Vec::new(),
        };
        Self {
            name,
            params,
            ret_type,
            blocks: vec![entry],
            next_val: 0,
            next_block: 1,
        }
    }

    pub fn fresh_val(&mut self) -> ValId {
        let id = ValId(self.next_val);
        self.next_val += 1;
        id
    }

    pub fn fresh_block(&mut self, name: &str) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            name: name.into(),
            insts: Vec::new(),
        });
        id
    }

    pub fn emit(&mut self, block: BlockId, inst: Inst, ty: IrType) -> ValId {
        let val = self.fresh_val();
        let bb = self.blocks.iter_mut().find(|b| b.id == block).unwrap();
        bb.insts.push((val, inst, ty));
        val
    }

    pub fn current_block(&self) -> BlockId {
        self.blocks.last().map(|b| b.id).unwrap_or(BlockId(0))
    }
}

/// An IR module — top-level container.
#[derive(Debug, Clone)]
pub struct IrModule {
    pub name: String,
    pub functions: Vec<IrFunction>,
    pub globals: Vec<(String, IrType, GlobalInit)>,
    pub metrics: Vec<String>, // metric names → global i64 counters
}

#[derive(Debug, Clone)]
pub enum GlobalInit {
    Int(i64),
    Str(String),
    Zero,
}

impl IrModule {
    pub fn new(name: String) -> Self {
        Self {
            name,
            functions: Vec::new(),
            globals: Vec::new(),
            metrics: Vec::new(),
        }
    }
}

impl fmt::Display for IrModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "; module: {}", self.name)?;

        for (name, ty, init) in &self.globals {
            writeln!(f, "@{name} = global {ty} {init:?}")?;
        }

        for metric in &self.metrics {
            writeln!(f, "@metric.{metric} = global i64 0")?;
        }

        for func in &self.functions {
            write!(f, "\nfn @{}(", func.name)?;
            for (i, (pname, pty)) in func.params.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{pname}: {pty}")?;
            }
            writeln!(f, ") -> {} {{", func.ret_type)?;

            for bb in &func.blocks {
                writeln!(f, "  {}:", bb.name)?;
                for (val, inst, ty) in &bb.insts {
                    writeln!(f, "    {val}: {ty} = {inst:?}")?;
                }
            }
            writeln!(f, "}}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_function() {
        let mut module = IrModule::new("test".into());
        let mut func = IrFunction::new("main".into(), vec![], IrType::I64);

        let entry = func.current_block();
        let v0 = func.emit(entry, Inst::ConstInt(42), IrType::I64);
        func.emit(entry, Inst::Ret(v0), IrType::Unit);

        module.functions.push(func);

        let ir_text = format!("{module}");
        assert!(ir_text.contains("fn @main"));
        assert!(ir_text.contains("ConstInt(42)"));
        assert!(ir_text.contains("Ret"));
    }

    #[test]
    fn test_metric_global() {
        let mut module = IrModule::new("test".into());
        module.metrics.push("requests".into());

        let ir_text = format!("{module}");
        assert!(ir_text.contains("@metric.requests"));
    }

    #[test]
    fn test_basic_block_creation() {
        let mut func = IrFunction::new("test".into(), vec![], IrType::Unit);
        let then_bb = func.fresh_block("then");
        let else_bb = func.fresh_block("else");
        assert_eq!(then_bb, BlockId(1));
        assert_eq!(else_bb, BlockId(2));
        assert_eq!(func.blocks.len(), 3); // entry + then + else
    }
}
