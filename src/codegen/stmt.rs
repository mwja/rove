//! Statement lowering: control flow, blocks, `if`, assignment and declarations.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    pub(super) fn lower_stmt(&mut self, stmt: ast::AstStmt) {
        match stmt.kind {
            ast::AstStmtKind::Expr(expr) | ast::AstStmtKind::BlockLike(expr) => {
                // discarded, so an owned result becomes a temp and is released
                self.lower_expr_borrowed(expr);
            }
            ast::AstStmtKind::Print(print) => self.lower_print(print),
            ast::AstStmtKind::Decl(decl) => self.lower_decl(decl),
            ast::AstStmtKind::Assign(assign) => self.lower_assign(assign),
            ast::AstStmtKind::ImplicitReturn(_) => {
                unreachable!("tails are lowered by lower_block_stmt as the block's value")
            }
            ast::AstStmtKind::Guard(guard_stmt) => self.lower_guard_stmt(guard_stmt),
            ast::AstStmtKind::Require(require) => {
                self.lower_aborting_constraint_param_passthrough(&ast::AstConstraint::Require(
                    require,
                ));
            }
            ast::AstStmtKind::Use(_) => {
                // use statements are lowered to nothing, they are only used for typeck
            }
        };

        if !self.is_current_block_terminated() {
            self.flush_temps();
        }
    }

    pub(super) fn lower_guard_stmt(&mut self, guard_stmt: ast::AstGuardStmt) {
        self.loc(&guard_stmt.node_id);
        let cond = self
            .lower_expr_borrowed(*guard_stmt.condition)
            .expect_scalar("guard condition must be scalar");
        let else_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(cond, continue_block, &[], else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        self.lower_block_stmt(guard_stmt.else_);
        if !self.is_current_block_terminated() {
            // typeck requires the else block to diverge, so this is unreachable
            // (e.g. the dead exit of an infinite `loop`).
            self.builder.ins().trap(TrapCode::unwrap_user(7));
        }

        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);
    }

    pub(super) fn lower_throw_stmt(&mut self, throw_stmt: ast::AstThrowStmt) {
        let err_value = self.lower_expr_borrowed(*throw_stmt.expr.clone());

        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("throw lowered outside of any scope")),
            ExitKind::Error,
        );

        self.flush_temps();
        self.builder.ins().jump(
            dest,
            &[BlockArg::Value(
                err_value.expect_scalar("error values must be scalar"),
            )],
        );
    }

    pub(super) fn lower_switch_stmt(&mut self, switch_stmt: ast::AstSwitchStmt) -> Operand {
        let ty = self
            .cx
            .body
            .node_ty(switch_stmt.node_id)
            .unwrap_or_else(|| bug!("a switch expression has no type"));
        let base_value = self.lower_expr_borrowed(*switch_stmt.expr.clone());

        let (_, else_, cases) = switch_stmt.consume();
        let (cases, bodies) = cases
            .into_iter()
            .map(|case| (case, self.builder.create_block()))
            .unzip::<_, _, Vec<_>, Vec<_>>();
        let else_block = else_.as_ref().map(|_| self.builder.create_block());
        let exit_block = self.value_block(&ty);
        // Without an else, typeck has checked every variant has a case.
        let default_block = else_block.unwrap_or_else(|| self.builder.create_block());

        // for each condition, evaluate then jump to the jumping block with the
        // right index.
        // it accepts an i32 of the index, as required by br_table.
        let switch_landing_pad = self.builder.create_block();
        self.builder
            .append_block_param(switch_landing_pad, types::I32);

        let jump_table = {
            let default_block_call = self.builder.func.dfg.block_call(default_block, &[]);
            let block_calls = bodies
                .iter()
                .map(|block| self.builder.func.dfg.block_call(*block, &[]))
                .collect::<Vec<_>>();

            self.builder
                .func
                .create_jump_table(JumpTableData::new(default_block_call, &block_calls))
        };

        // an idx we know will trigger the default case
        let safe_default_idx = self.builder.ins().iconst(types::I32, cases.len() as i64);

        // for each condition, make another block that then branches to the
        // switch landing pad.
        let check_blocks = cases
            .iter()
            .map(|_| self.builder.create_block())
            .collect::<Vec<_>>();
        for (idx, case) in cases.iter().enumerate() {
            let block = check_blocks[idx];
            if !self.is_current_block_terminated() {
                self.builder.ins().jump(block, &[]);
            }
            self.builder.switch_to_block(block);
            let value = self.lower_expr_borrowed(*case.expr.clone());
            // check equality, insert a fake astnode for the condition
            let condition = self.lower_cond_direct(
                self.cx
                    .body
                    .node_ty(case.expr.node_id())
                    .unwrap_or_else(|| bug!("a switch case expression has no type")),
                base_value.expect_scalar("switch target must be scalar"),
                value.expect_scalar("switch case must be scalar"),
                self.cx.tcx.int_ty(),
                ast::AstBinaryOperator::Eq,
            );
            let idx_val = self.builder.ins().iconst(types::I32, idx as i64);
            let (fallback_target, fallback_params) = check_blocks
                .get(idx + 1)
                .map(|b| (*b, None))
                .unwrap_or_else(|| (switch_landing_pad, Some(BlockArg::Value(safe_default_idx))));
            self.builder.ins().brif(
                condition,
                switch_landing_pad,
                &[BlockArg::Value(idx_val)],
                fallback_target,
                fallback_params
                    .as_ref()
                    .map(|p| std::slice::from_ref(p))
                    .unwrap_or(&[]),
            );
            self.builder.seal_block(block);
        }

        // above we fell through to the different blocks, now let's make the switch landing pad.
        if !self.is_current_block_terminated() {
            self.builder.ins().jump(switch_landing_pad, &[]);
        }
        self.builder.switch_to_block(switch_landing_pad);
        self.builder.seal_block(switch_landing_pad);

        let idx = self.builder.block_params(switch_landing_pad)[0];
        self.builder.ins().br_table(idx, jump_table);

        // then for each case_block, we should just execute the statements then
        // jump to the exit block.
        for (idx, (body, block)) in cases.into_iter().zip(bodies.iter()).enumerate() {
            self.builder.switch_to_block(*block);
            let scope_id = self
                .cx
                .body
                .node_scope(body.body.node_id)
                .unwrap_or_else(|| bug!("a block has no associated scope"));
            self.switches.push(SwitchInfo {
                scope: scope_id,
                current_block: *block,
                upcoming_block: bodies.get(idx + 1).copied().or(else_block),
                exit: exit_block,
            });
            let value = self.lower_block_stmt(body.body);
            self.switches.pop();
            self.yield_to(exit_block, value);
            self.builder.seal_block(*block);
        }

        if let (Some(else_), Some(else_block)) = (else_, else_block) {
            self.builder.switch_to_block(else_block);
            let value = self.lower_block_stmt(else_.body);
            self.yield_to(exit_block, value);
            self.builder.seal_block(else_block);
        } else {
            self.builder.switch_to_block(default_block);
            self.builder.seal_block(default_block);
            self.builder.ins().trap(TrapCode::unwrap_user(7));
        }

        self.finish_value_block(exit_block, &ty)
    }

    pub(super) fn lower_fallthrough_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("fallthrough lowered outside of any scope")),
            ExitKind::Fallthrough,
        );
        self.flush_temps();
        self.builder.ins().jump(dest, &[]);
    }

    pub(super) fn lower_break_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("break lowered outside of any scope")),
            ExitKind::Break,
        );
        self.flush_temps();
        self.builder.ins().jump(dest, &[]);
    }

    pub(super) fn lower_continue_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("continue lowered outside of any scope")),
            ExitKind::Continue,
        );
        self.builder.ins().jump(dest, &[]);
    }

    pub(super) fn lower_gen_loop(&mut self, body: ast::AstBlockStmt, cond: Option<ast::AstExpr>) {
        let scope_id = self
            .cx
            .body
            .node_scope(body.node_id)
            .unwrap_or_else(|| bug!("a block has no associated scope"));

        // Create relevant header blocks (condition checks)
        let header_block = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        // Entry edge. Header is NOT sealed: the back-edge doesn't exist yet.
        self.builder.ins().jump(header_block, &[]);
        self.builder.switch_to_block(header_block);
        match cond {
            Some(c) => {
                let v = self
                    .lower_expr_borrowed(c)
                    .expect_scalar("loop condition must be scalar"); // outer scope: frame not pushed yet
                self.builder.ins().brif(v, body_block, &[], exit_block, &[]);
            }
            None => {
                self.builder.ins().jump(body_block, &[]);
            }
        }
        self.builder.seal_block(body_block); // header is its only predecessor
        self.builder.switch_to_block(body_block);
        self.loops.push(LoopInfo {
            scope: scope_id,
            header: header_block,
            exit: exit_block,
        });

        self.lower_block_stmt(body);

        // Falling off the end is a continue: releases inline, then the back-edge.
        if !self.is_current_block_terminated() {
            self.builder.ins().jump(header_block, &[]);
        }
        self.loops.pop();
        // Every continue is emitted, so every header predecessor now exists.
        self.builder.seal_block(header_block);
        self.builder.seal_block(exit_block);

        self.builder.switch_to_block(exit_block);
    }

    pub(super) fn lower_return_stmt(&mut self, return_: ast::AstReturnStmt) {
        let args: Vec<BlockArg> = match (return_.expr, self.chain_repr(ExitKind::Return)) {
            (None, Repr::Empty) => vec![],
            (Some(e), Repr::Scalar(_)) => vec![BlockArg::Value(
                self.lower_expr_owned(*e)
                    .expect_scalar("cannot return pre-made fallible type (yet)"),
            )],
            (e, r) => unreachable!("return arity mismatch: expr={}, repr={r:?}", e.is_some()),
        };
        let target = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("return lowered outside of any scope")),
            ExitKind::Return,
        );
        self.flush_temps();
        self.builder.ins().jump(target, &args);
    }

    /// Jumps to `target` with a block's value, unless the block already jumped
    /// elsewhere.
    pub(super) fn yield_to(&mut self, target: Block, value: Operand) {
        if self.is_current_block_terminated() {
            return;
        }
        let args: Vec<BlockArg> = match value {
            Operand::Empty => vec![],
            Operand::Scalar(v) => vec![BlockArg::Value(v)],
            Operand::Pair(..) => unreachable!("blocks cannot yield fallible values"),
        };
        if args.len() != self.builder.block_params(target).len() {
            // typeck only lets a block skip its value if it diverges (e.g. an
            // infinite `loop`), so this point is unreachable.
            self.builder.ins().trap(TrapCode::unwrap_user(7));
            return;
        }
        self.builder.ins().jump(target, &args);
    }

    /// Returns the block's value, which is its tail (if any).
    pub(super) fn lower_block_stmt(&mut self, block: ast::AstBlockStmt) -> Operand {
        self.loc(&block.node_id);
        let scope_id = self
            .cx
            .body
            .node_scope(block.node_id)
            .unwrap_or_else(|| bug!("a block has no associated scope"));

        // open scope frame for this block.
        self.open_frame(scope_id);
        self.emit_frame_locals_declare(scope_id);

        let mut value = Operand::Empty;
        for stmt in block.stmts {
            match stmt.kind {
                // typeck only permits a tail as the last statement.
                // owned so it outlives this frame's locals being released.
                ast::AstStmtKind::ImplicitReturn(expr) => value = self.lower_expr_owned(expr),
                kind => self.lower_stmt(ast::AstStmt { kind }),
            }
        }

        // Got quite a few of these through the file for the minute but I'm not
        // really sure how to consistently impl that.
        if !self.is_current_block_terminated() {
            self.emit_frame_locals_release(scope_id);
        }

        self.close_frame(scope_id);
        value
    }

    pub(super) fn lower_if(&mut self, if_expr: ast::AstIfStmt) -> Operand {
        let ty = self
            .cx
            .body
            .node_ty(if_expr.node_id)
            .unwrap_or_else(|| bug!("an if expression has no type"));
        let merge_block = self.value_block(&ty);
        self.lower_if_into(if_expr, merge_block);
        self.finish_value_block(merge_block, &ty)
    }

    /// Lowers an if whose value goes to `merge_block`, which an `else if`
    /// shares with the `if` it follows.
    pub(super) fn lower_if_into(&mut self, if_expr: ast::AstIfStmt, merge_block: Block) {
        self.loc(&if_expr.node_id);
        let then_block = self.builder.create_block();
        let else_block = if let Some(_) = if_expr.else_ {
            Some(self.builder.create_block())
        } else {
            None
        };
        let cond = self
            .lower_expr_borrowed(*if_expr.cond)
            .expect_scalar("cannot return pre-made fallible type (yet)");
        // Without an else the if is void, so the merge block takes no value.
        self.builder.ins().brif(
            cond,
            then_block,
            &[],
            else_block.unwrap_or(merge_block),
            &[],
        );

        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        let value = self.lower_block_stmt(*if_expr.then);
        self.yield_to(merge_block, value);

        if let Some(else_) = if_expr.else_ {
            let else_block = else_block.unwrap_or_else(|| {
                bug!("if statement has an else branch but no else block was created")
            });
            self.builder.switch_to_block(else_block);
            self.builder.seal_block(else_block);
            match else_ {
                ast::AstElseBranch::Block(block_stmt) => {
                    let value = self.lower_block_stmt(block_stmt);
                    self.yield_to(merge_block, value);
                }
                ast::AstElseBranch::If(if_expr) => self.lower_if_into(*if_expr, merge_block),
            }
        }
    }

    /// A block that branches of a value of type `ty` (e.g. an if's) yield to.
    pub(super) fn value_block(&mut self, ty: &Ty) -> Block {
        let block = self.builder.create_block();
        for param_ty in self.rcx.repr_of(ty).types() {
            self.builder.append_block_param(block, param_ty);
        }
        block
    }

    /// Continues from a [value_block] once every branch has yielded to it,
    /// returning the value.
    pub(super) fn finish_value_block(&mut self, block: Block, ty: &Ty) -> Operand {
        self.builder.switch_to_block(block);
        self.builder.seal_block(block);
        if *ty.kind() == TyKind::Never {
            // Every branch diverges, so this block is dead code with no
            // predecessors, but Cranelift still requires a terminator.
            self.builder.ins().trap(TrapCode::unwrap_user(7));
        }
        match self.builder.block_params(block) {
            [] => Operand::Empty,
            [v] => Operand::Scalar(*v),
            _ => unreachable!("block values are single-slot"),
        }
    }

    pub(super) fn lower_assign(&mut self, assign: ast::AstAssignStmt) {
        self.loc(&assign.node_id);

        let assign_expr_node_id = assign.expr.node_id();

        let value = self
            .lower_expr_owned(*assign.expr)
            .expect_scalar("cannot store fallible types in variables (yet)");

        let res = self
            .cx
            .body
            .node_res(assign.name.node_id())
            .unwrap_or_else(|| bug!("a place has no resolution"));

        match res {
            res @ Res::Local(_) => self.lower_simple_assign(res, value),
            Res::Field {
                base,
                field,
                struct_id,
            } => self.lower_field_assign(*base, &field, struct_id, value),
            Res::Param(..)
            | Res::Def(..)
            | Res::ConstraintOld(..)
            | Res::ConstraintRet
            | Res::Enum(..)
            | Res::EnumVariant(..)
            | Res::Struct(..)
            | Res::StructField(..)
            | Res::Module(..) => bug!("attempted to assign to non-assignable value"),
        }
    }

    pub(super) fn lower_simple_assign(&mut self, res: Res, value: Value) {
        let Res::Local(local_id) = res else {
            unreachable!()
        };
        let ty = self
            .cx
            .body
            .local_ty(local_id)
            .unwrap_bug("local has no type");
        let target = self.locals[&local_id];
        let old = ty.is_ref_counted().then(|| self.builder.use_var(target));
        self.builder.def_var(target, value);
        if let Some(old) = old {
            self.emit_release(old);
        }
    }

    pub(super) fn lower_field_assign(
        &mut self,
        base: Res,
        field: &str,
        struct_id: StructId,
        value: Value,
    ) {
        // here...
        let base = self.lower_value_res(base);
        let struct_ = self
            .cx
            .tcx
            .structs
            .get_struct(struct_id)
            .unwrap_or_else(|| bug!("unable to resolve struct id for field assignment"));

        let layout = self.get_cached_layout(struct_id);
        let idx = struct_.get_field_index(field).unwrap_or_else(|| {
            bug!("unable to find field {} in struct {}", field, struct_.name);
        });
        let f = layout.fields.get(idx).unwrap_or_else(|| {
            bug!(
                "unable to find field {} in struct {} layout",
                field,
                struct_.name
            );
        });

        let base = base.expect_scalar("must be scalar to store into field");

        // read the old value before overwriting, if it's managed.
        let old = f.managed.then(|| {
            self.builder
                .ins()
                .load(f.ty, MemFlagsData::new(), base, f.offset as i32)
        });

        // Store the new value in the field
        self.builder
            .ins()
            .store(MemFlagsData::new(), value, base, f.offset as i32);

        // Release the old value.
        if let Some(old) = old {
            self.emit_release(old);
        }
    }

    pub(super) fn lower_decl(&mut self, decl: ast::AstDecl) {
        match decl.kind {
            ast::AstDeclKind::Let(let_decl) => self.lower_let(let_decl),
        }
    }

    pub(super) fn lower_let(&mut self, let_decl: ast::AstLetDecl) {
        self.loc(&let_decl.name.node_id);

        let Res::Local(local_id) = self
            .cx
            .body
            .node_res(let_decl.name.node_id)
            .unwrap_or_else(|| bug!("a let declaration has no resolution"))
        else {
            unreachable!();
        };
        let value = self
            .lower_expr_owned(*let_decl.expr)
            .expect_scalar("cannot store fallible types in variables (yet)");
        // for now we only have i64s... so no need to check
        let variable = self
            .locals
            .get(&local_id)
            .copied()
            .unwrap_or_else(|| bug!("a let-declared local has no cranelift variable"));
        self.builder.def_var(variable, value);

        self.insert(Res::Local(local_id), variable);
    }

    pub(super) fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.loc(&print.node_id);
        let x_ty = self
            .cx
            .body
            .node_ty(print.expr.node_id())
            .unwrap_or_else(|| bug!("a print expression has no type"));
        let x = self
            .lower_expr_borrowed(*print.expr)
            .expect_scalar("cannot print fallible type");
        // enum values are stored as index + 1 (0 is reserved for "no error"),
        // but print the variant's index.
        let x = match x_ty.kind() {
            TyKind::Enum(..) => self.builder.ins().iadd_imm_s(x, -1),
            _ => x,
        };
        let printf_ref = self.get_or_cache_function(match x_ty.kind() {
            TyKind::Int | TyKind::Enum(..) => self.runtime.println_i64,
            TyKind::Float => self.runtime.println_f64,
            TyKind::Void | TyKind::Never => unreachable!(),
            TyKind::Func(_) => unreachable!(),
            TyKind::FullFallible(_, _) => unreachable!(),
            TyKind::Bool => self.runtime.println_bool,
            TyKind::Str => self.runtime.println_str,
            TyKind::String => self.runtime.println_string,
            TyKind::Struct(_) => unreachable!(),
        });
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }
}
