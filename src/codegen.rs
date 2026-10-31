//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

mod arc;
mod call;
mod cond;
mod constraint;
mod expr;
mod layout;
mod object;
mod ownership;
mod repr;
mod runtime;
mod scope;
mod stmt;
use cranelift::{
    codegen::{
        cfg_printer::CFGPrinter,
        ir::{
            AbiParam, Block, BlockArg, BlockCall, FuncRef, GlobalValue, InstBuilder, JumpTable,
            JumpTableData, MemFlags, MemFlagsData, SigRef, Signature, SourceLoc, StackSlotData,
            StackSlotKind, TrapCode, Type, UserExternalName, Value,
            condcodes::{FloatCC, IntCC},
            types,
        },
        isa::TargetFrontendConfig,
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
    module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};
use object::*;
pub use object::{CodegenOptions, generate_object};
use ownership::Ownership::{self, *};
use runtime::Runtime;
use std::{collections::HashMap, error::Error, io::Write as _, mem, path::PathBuf, rc::Rc};

use crate::{
    UnwrapBug,
    ast::{self, AstDef, AstForcedTryExpr, NodeId},
    codegen::{
        layout::{StructLayout, calculate_layout},
        repr::{Repr, ReprCx},
    },
    defs::{self, Def, DefKind, FuncSig},
    mangle,
    sourcemap::{SourceMap, SpanRecorder},
    structs::{Struct, StructId},
    ty::{
        Ty, TyCtxt, TyKind,
        res::{LocalId, OldId, Res},
        typeck::{BodyInfo, ScopeId},
    },
};

#[derive(Clone, Copy)]
pub struct SourceInfo<'s> {
    pub source_map: &'s SourceMap,
    pub node_to_span: &'s SpanRecorder<NodeId>,
}

impl SourceInfo<'_> {
    /// The 1-based line a node starts on, or 0 if it has no known span.
    fn line(&self, node_id: NodeId) -> u32 {
        self.node_to_span
            .get(&node_id)
            .and_then(|span| self.source_map.span_start_line(*span))
            .unwrap_or(0) as u32
    }
}

pub struct CompilerCtxt<'c> {
    pub tcx: &'c TyCtxt,
    pub body: &'c BodyInfo,
    pub source: SourceInfo<'c>,
    def_id_to_function_id: &'c HashMap<defs::DefId, FuncId>,
}

impl<'c> CompilerCtxt<'c> {
    pub fn new(
        tcx: &'c TyCtxt,
        body: &'c BodyInfo,
        source: SourceInfo<'c>,
        table: &'c HashMap<defs::DefId, FuncId>,
    ) -> Self {
        Self {
            tcx,
            body,
            source,
            def_id_to_function_id: table,
        }
    }

    pub fn def_id_to_function_id(&self, def_id: defs::DefId) -> Option<FuncId> {
        self.def_id_to_function_id.get(&def_id).copied()
    }
}

fn func_ty(om: &ObjectModule) -> Type {
    om.target_config().pointer_type()
}

enum ShortCircuit {
    And,
    Or,
}

#[derive(Copy, Clone, Debug)]
enum Operand {
    Empty,
    Scalar(Value),
    /// (payload, tag). tag != 0 means the call errored — payload may be garbage.
    Pair(Value, Value),
}

impl Operand {
    fn expect_scalar(self, what: &str) -> Value {
        match self {
            Operand::Scalar(v) => v,
            other => unreachable!("{what} must be single-slot, got {other:?}"),
        }
    }

    fn owned(self) -> (Self, Ownership) {
        (self, Owned)
    }

    fn borrowed(self) -> (Self, Ownership) {
        (self, Borrowed)
    }
}

fn lower_sig(rcx: ReprCx, def_sig: &FuncSig, is_main: bool) -> Signature {
    let mut sig = rcx.make_signature();
    for param in &def_sig.param_tys {
        sig.params
            .extend(rcx.repr_of(param).types().map(AbiParam::new));
    }

    if is_main {
        // main is exported to the runtime as `__rove_entry` under a fixed
        // `() -> i64` ABI, regardless of the source return type; rt_start
        // narrows to i32 and supplies 0 for a void main.
        sig.returns.push(AbiParam::new(types::I64));
    } else {
        sig.returns.extend(
            rcx.fn_return_repr(&def_sig.return_ty, def_sig.throws_ty.as_ref())
                .types()
                .map(AbiParam::new),
        );
    }

    sig
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ExitKind {
    Continue,
    Break,
    Return,
    Error,
    /// fallthrough for SWITCHES.
    Fallthrough,
}
impl ExitKind {
    const COUNT: usize = 5;
    fn idx(self) -> usize {
        match self {
            ExitKind::Continue => 0,
            ExitKind::Break => 1,
            ExitKind::Return => 2,
            ExitKind::Error => 3,
            ExitKind::Fallthrough => 4,
        }
    }
}

struct ScopeFrame {
    scope: ScopeId,
    parent: Option<ScopeId>,
    links: [Option<Block>; ExitKind::COUNT],
}

struct LoopInfo {
    scope: ScopeId,
    header: Block,
    exit: Block,
}

struct SwitchInfo {
    scope: ScopeId,
    current_block: Block,
    // index 0 is next block, index 1 is one after next, etc.
    upcoming_block: Option<Block>,
    exit: Block,
}

#[derive(Clone, Copy)]
enum BlockType {
    Regular,
    PostConstraint,
}

struct CraneliftCodegen<'a, 'o> {
    module: &'o mut ObjectModule,
    builder: &'o mut FunctionBuilder<'a>,
    runtime: Runtime,
    locals: HashMap<LocalId, Variable>,
    old_values: HashMap<OldId, Operand>,
    cached_functions: HashMap<FuncId, FuncRef>,
    cached_signatures: HashMap<FuncSig, SigRef>,
    cached_layouts: HashMap<StructId, Rc<StructLayout>>,
    cached_tinfo: HashMap<StructId, DataId>,
    // Temporary values to release on statement end (borrowed ones.)
    temps: Vec<Value>,
    // Used to retrieve parameters from the entry block (hence function)
    entry_block: Block,
    success_landing_pad: Block,
    failure_landing_pad: Block,
    /// The landing pad is the final block of the whole function, errors may be
    /// handled here in the future, etc.
    landing_pad: Block,
    block_type: BlockType,
    is_main: bool,

    // Scope frames
    current_scope: Option<ScopeId>,
    scope_frames: HashMap<ScopeId, ScopeFrame>,
    loops: Vec<LoopInfo>,
    switches: Vec<SwitchInfo>,
    cx: CompilerCtxt<'o>,
    rcx: ReprCx,

    // for debug/errors stuff
    fn_name: String,
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    // Likely to change...
    fn lookup_local(&self, res: &Res) -> Option<Variable> {
        match res {
            Res::Local(local_id) => self.locals.get(local_id).copied(),
            _ => None,
        }
    }

    fn get_cached_layout(&mut self, struct_id: StructId) -> Rc<StructLayout> {
        if let Some(layout) = self.cached_layouts.get(&struct_id) {
            return layout.clone();
        }

        let struct_ = self
            .cx
            .tcx
            .structs
            .get_struct(struct_id)
            .unwrap_or_else(|| bug!("unable to find struct for struct_id {struct_id:?}"));

        let layout = calculate_layout(&self.rcx, struct_);
        let rc_layout = Rc::new(layout);
        self.cached_layouts.insert(struct_id, rc_layout.clone());
        rc_layout
    }

    fn lower_sig_cached(&mut self, def_sig: FuncSig) -> SigRef {
        if let Some(signature) = self.cached_signatures.get(&def_sig) {
            return *signature;
        }

        let sig = lower_sig(self.rcx, &def_sig, false);
        let sig_ref = self.builder.import_signature(sig);

        self.cached_signatures.insert(def_sig, sig_ref);

        sig_ref
    }

    fn insert(&mut self, res: Res, var: Variable) {
        match res {
            Res::Local(local_id) => self.locals.insert(local_id, var),
            _ => None,
        };
    }

    fn get_or_cache_function(&mut self, func_id: FuncId) -> FuncRef {
        if let Some(func) = self.cached_functions.get(&func_id) {
            return func.clone();
        }
        let func_ref = self
            .module
            .declare_func_in_func(func_id, &mut self.builder.func);

        self.cached_functions.insert(func_id, func_ref);
        func_ref
    }

    fn loc(&mut self, node_id: &NodeId) {
        self.builder
            .set_srcloc(SourceLoc::new(node_id.index() as u32));
    }

    fn frame(&self, scope: ScopeId) -> &ScopeFrame {
        self.scope_frames
            .get(&scope)
            .unwrap_or_else(|| bug!("no scope frame exists for the requested scope"))
    }

    fn frame_mut(&mut self, scope: ScopeId) -> &mut ScopeFrame {
        self.scope_frames
            .get_mut(&scope)
            .unwrap_or_else(|| bug!("no scope frame exists for the requested scope"))
    }
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    fn lower(mut self, def: ast::AstFunctionDef) {
        // Must pre-lower the expressions for the post-checks.
        self.lower_constraint_olds(&def.constraints);

        // Then the require checks (ensure checks happen in the success pad and branch to the landing pad if failing)
        self.lower_constraints_require(&def.constraints);
        self.lower_constraints_guards(&def.constraints);
        let value = self.lower_block_stmt(def.body);
        self.yield_to(self.success_landing_pad, value);

        self.build_success_pad(&def.constraints);
        self.build_failure_pad();
        // build landing pad
        if self.is_main {
            self.build_main_landing_pad();
        } else {
            self.build_landing_pad();
        }
    }

    fn is_current_block_terminated(&mut self) -> bool {
        self.is_block_terminated(
            self.builder
                .current_block()
                .unwrap_or_else(|| bug!("builder has no current block")),
        )
    }

    fn is_block_terminated(&mut self, block: Block) -> bool {
        if let Some(last_inst) = self.builder.func.layout.last_inst(block) {
            let opcode = self.builder.func.dfg.insts[last_inst].opcode();
            return opcode.is_terminator();
        }
        false
    }

    fn fallthrough_to(&mut self, pad: Block) {
        if self.is_current_block_terminated() {
            return;
        }
        debug_assert_eq!(
            self.rcx.repr_of(&self.cx.body.return_ty),
            Repr::Empty,
            "non-void function fell off the end; typeck should have reported a missing return",
        );
        self.builder.ins().jump(pad, &[]);
    }
}
