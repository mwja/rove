//! Object file generation: declaring and defining every function in the module.

use super::*;

#[derive(Default)]
pub struct CodegenOptions {
    /// Emit the raw CLIF to the given path.
    pub emit_clif_to: Option<PathBuf>,
    /// Emit the cranelift optimized CLIF to the given path.
    pub emit_opt_clif_to: Option<PathBuf>,
    /// Emit a DOT graph of the control flow graph to the given path.
    pub emit_cfg_to: Option<PathBuf>,
}

pub fn generate_object(
    tcx: &mut TyCtxt,
    ast: ast::AstModule,
    source: SourceInfo,
    options: Option<CodegenOptions>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let options = options.unwrap_or_default();

    let isa_builder =
        cranelift::native::builder().unwrap_or_else(|e| bug!("unable to create a native isa: {e}"));
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true")?;
    flag_builder.set("opt_level", "speed_and_size")?;

    let flags = settings::Flags::new(flag_builder);

    let isa = isa_builder.finish(flags)?;

    let object_builder = ObjectBuilder::new(isa, "program", default_libcall_names())
        .unwrap_or_else(|e| bug!("unable to create an object builder for the program: {e}"));
    let mut module = ObjectModule::new(object_builder);

    let rcx = ReprCx::new(module.target_config());
    let mut table = HashMap::new();
    let mut func_sig = HashMap::new();

    compile_module(tcx, &ast, &mut module, rcx, &mut table, &mut func_sig)?;

    // Builtins are implemented by the runtime, so are only imported.
    for builtin in tcx.builtins.values() {
        let Some(Def {
            kind: DefKind::Function(sig),
        }) = tcx.defs.def(builtin.def_id)
        else {
            bug!("a builtin has no function def");
        };
        let id = module.declare_function(
            builtin.symbol,
            Linkage::Import,
            &lower_sig(rcx, sig, false),
        )?;
        table.insert(builtin.def_id, id);
    }

    let emit_clif_file = match &options.emit_clif_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit clif: {e}")),
        ),
        None => None,
    };
    let emit_opt_clif_file = match &options.emit_opt_clif_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit optimized clif: {e}")),
        ),
        None => None,
    };
    let emit_cfg_file = match &options.emit_cfg_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit a DOT diagram: {e}")),
        ),
        None => None,
    };

    let runtime = Runtime::from_object(&mut module);
    let mut ctx = module.make_context();
    let mut emit = EmitFiles {
        clif: emit_clif_file,
        opt_clif: emit_opt_clif_file,
        cfg: emit_cfg_file,
    };
    define_module(
        tcx,
        ast,
        source,
        &mut module,
        &mut ctx,
        runtime,
        rcx,
        &table,
        &mut func_sig,
        &mut emit,
    )?;

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(bytes)
}

pub(super) fn compile_module(
    tcx: &mut TyCtxt,
    ast: &ast::AstModule,
    module: &mut ObjectModule,
    rcx: ReprCx,
    table: &mut HashMap<defs::DefId, FuncId>,
    func_sig: &mut HashMap<defs::DefId, Signature>,
) -> Result<(), Box<dyn Error + 'static>> {
    let module_id = tcx
        .mcx
        .get_module_id(ast.node_id)
        .unwrap_or_else(|| bug!("unable to resolve module id of module in codegen"));

    for def in ast.defs.iter() {
        match def {
            ast::AstDef::Function(func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(func_def.node_id)
                    .unwrap_or_else(|| {
                        bug!("a function definition was unable to be resolved to a def_id")
                    });
                let def = tcx.defs.def(def_id);
                let Some(Def {
                    kind: DefKind::Function(sig),
                }) = def
                else {
                    continue;
                };

                let is_main = tcx.is_entry_point(def_id);
                let sig = lower_sig(rcx, sig, is_main);
                let symbol = mangle::mangle_name(tcx, module_id, &func_def.name);

                let id = module.declare_function(
                    if is_main { "__rove_entry" } else { &symbol },
                    if is_main {
                        Linkage::Export
                    } else {
                        Linkage::Local
                    },
                    &sig,
                )?;
                func_sig.insert(def_id, sig);
                table.insert(def_id, id);
            }
        }
    }

    for ast in ast.mods.iter() {
        compile_module(tcx, ast, module, rcx, table, func_sig)?;
    }

    Ok(())
}

/// Files that debug artifacts are written to while defining functions.
pub(super) struct EmitFiles {
    clif: Option<std::fs::File>,
    opt_clif: Option<std::fs::File>,
    cfg: Option<std::fs::File>,
}

/// Lowers and defines every function in the module (and its child modules)
/// that was declared by `compile_module`.
pub(super) fn define_module(
    tcx: &TyCtxt,
    ast: ast::AstModule,
    source: SourceInfo,
    module: &mut ObjectModule,
    ctx: &mut cranelift::codegen::Context,
    runtime: Runtime,
    rcx: ReprCx,
    table: &HashMap<defs::DefId, FuncId>,
    func_sig: &mut HashMap<defs::DefId, Signature>,
    emit: &mut EmitFiles,
) -> Result<(), Box<dyn Error + 'static>> {
    let module_id = tcx
        .mcx
        .get_module_id(ast.node_id)
        .unwrap_or_else(|| bug!("unable to resolve module id of module in codegen"));

    for def in ast.defs {
        let AstDef::Function(func_def) = def else {
            continue;
        };

        let def_id = tcx
            .defs
            .resolve_def_id_for_node_id(func_def.node_id)
            .unwrap_or_else(|| bug!("a function definition was unable to be resolved to a def_id when lowering to cranelift"));
        let id = table
            .get(&def_id)
            .unwrap_or_else(|| bug!("a function def_id has no cranelift function id"));
        ctx.func.name = cranelift::codegen::ir::UserFuncName::User(UserExternalName {
            namespace: 0,
            index: id.as_u32(),
        });

        let mut func_ctx = FunctionBuilderContext::new();

        let cx = CompilerCtxt::new(
            tcx,
            tcx.bodies
                .get(&def_id)
                .unwrap_or_else(|| bug!("a function def_id has no typechecked body")),
            source,
            &table,
        );
        ctx.func.signature = func_sig
            .remove(&def_id)
            .unwrap_or_else(|| bug!("a function def_id has no lowered signature"));

        let mut fbuilder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        // disable in release builds of apps.
        fbuilder.func.stencil.dfg.collect_debug_info();

        let block = fbuilder.create_block();
        fbuilder.append_block_params_for_function_params(block);

        let landing_pad = fbuilder.create_block();
        for ty in rcx
            .fn_return_repr(&cx.body.return_ty, cx.body.throws_ty.as_ref())
            .types()
        {
            fbuilder.append_block_param(landing_pad, ty);
        }

        let success_landing_pad = fbuilder.create_block();
        for ty in rcx.success_repr_of(&cx.body.return_ty).types() {
            fbuilder.append_block_param(success_landing_pad, ty);
        }

        let failure_landing_pad = fbuilder.create_block();
        if let Some(throws_ty) = &cx.body.throws_ty {
            let err_ty = rcx
                .repr_of(throws_ty)
                .expect_scalar("only scalars may be thrown");
            fbuilder.append_block_param(failure_landing_pad, err_ty);
        }

        // // the abort block just calls the runtime abort and exits, it exists
        // // to avoid code duplication. it shares the same params as the abort function
        // // so that the runtime abort call can be shared.
        // //
        // // literally copy it one-for-one.
        // let condition_abort_block = fbuilder.create_block();
        // for param in &module
        //     .declarations()
        //     .get_function_decl(runtime.abort_constraint)
        //     .signature
        //     .params
        // {
        //     fbuilder.append_block_param(condition_abort_block, param.value_type);
        // }

        // go back to main block.
        fbuilder.switch_to_block(block);
        fbuilder.seal_block(block);
        let codegen = CraneliftCodegen {
            runtime,
            module: &mut *module,
            builder: &mut fbuilder,
            locals: HashMap::new(),
            old_values: HashMap::new(),
            cached_functions: HashMap::new(),
            cached_signatures: HashMap::new(),
            cached_layouts: HashMap::new(),
            cached_tinfo: HashMap::new(),
            temps: Vec::new(),
            entry_block: block,
            success_landing_pad,
            failure_landing_pad,
            block_type: BlockType::Regular,
            landing_pad,
            is_main: tcx.is_entry_point(def_id),
            current_scope: None,
            scope_frames: HashMap::new(),
            loops: Vec::new(),
            switches: Vec::new(),
            fn_name: mangle::qualified_name(tcx, module_id, &func_def.name),
            cx,
            rcx,
        };

        codegen.lower(func_def);

        if let Some(file) = &mut emit.cfg {
            writeln!(file, "{}", CFGPrinter::new(fbuilder.func).to_string())
                .unwrap_or_else(|e| bug!("unable to write the DOT diagram to its file: {e}"));
        }

        fbuilder.finalize(module.target_config());

        if let Some(file) = &mut emit.clif {
            writeln!(file, "{}", ctx.func.display())
                .unwrap_or_else(|e| bug!("unable to write clif to its file: {e}"));
        }

        module.define_function(*id, ctx)?;

        if let Some(file) = &mut emit.opt_clif {
            writeln!(file, "{}", ctx.func.display())
                .unwrap_or_else(|e| bug!("unable to write optimized clif to its file: {e}"));
        }

        module.clear_context(ctx);
    }

    for child in ast.mods {
        define_module(
            tcx, child, source, module, ctx, runtime, rcx, table, func_sig, emit,
        )?;
    }

    Ok(())
}

pub(super) fn iter_u32_to_u8(data: impl IntoIterator<Item = u32> + ExactSizeIterator) -> Box<[u8]> {
    let mut u8_data = Vec::with_capacity(data.len() * 4);
    for val in data {
        u8_data.extend_from_slice(&val.to_le_bytes());
    }
    u8_data.into_boxed_slice()
}
