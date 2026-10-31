//! Expression lowering: paths, fields, struct init, operators and literals.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    pub(super) fn lower_expr(&mut self, expr: ast::AstExpr) -> (Operand, Ownership) {
        self.loc(&expr.node_id());

        match expr.kind {
            ast::AstExprKind::Literal(_) => Operand::Scalar(self.lower_lit(expr)).borrowed(),
            ast::AstExprKind::Unary(unary_expr) => {
                Operand::Scalar(self.lower_unary_expr(unary_expr)).borrowed()
            }
            ast::AstExprKind::Binary(_) => Operand::Scalar(self.lower_bin_expr(expr)).borrowed(),
            ast::AstExprKind::Ident(ident) => self.lower_ident(ident).borrowed(),
            ast::AstExprKind::Call(call_expr) => match self.cx.body.node_res(call_expr.node_id) {
                Some(res @ Res::ConstraintOld(_)) => self.lower_value_res(res).borrowed(),
                _ => self.lower_call(call_expr),
            },
            ast::AstExprKind::Path(path_expr) => self.lower_path(path_expr).borrowed(),
            ast::AstExprKind::ImplicitPath(implicit_path_expr) => {
                self.lower_implicit_path(implicit_path_expr).borrowed()
            }
            ast::AstExprKind::ForcedTry(forced_try_expr) => self.lower_forced_try(forced_try_expr),
            ast::AstExprKind::TryCatch(try_catch_expr) => self.lower_try_catch(try_catch_expr),
            ast::AstExprKind::Block(block) => self.lower_block_stmt(block).owned(),
            // every branch yields a block tail, which is owned
            ast::AstExprKind::If(if_expr) => self.lower_if(if_expr).owned(),
            ast::AstExprKind::Switch(switch_expr) => self.lower_switch_stmt(switch_expr).owned(),
            ast::AstExprKind::Loop(loop_expr) => {
                self.lower_gen_loop(loop_expr.body, None);
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::While(while_expr) => {
                self.lower_gen_loop(while_expr.body, Some(*while_expr.cond));
                Operand::Empty.borrowed()
            }
            // Anything that goes away is never (typeck)
            ast::AstExprKind::Return(return_expr) => {
                self.lower_return_stmt(return_expr);
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::Throw(throw_expr) => {
                self.lower_throw_stmt(throw_expr);
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::Break(_) => {
                self.lower_break_stmt();
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::Continue(_) => {
                self.lower_continue_stmt();
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::Fallthrough(_) => {
                self.lower_fallthrough_stmt();
                Operand::Empty.borrowed()
            }
            ast::AstExprKind::StructInit(struct_init) => self.lower_struct_init(struct_init),
            ast::AstExprKind::FieldAccess(field_access) => {
                self.lower_field_access(field_access).borrowed()
            }
        }
    }

    pub(super) fn lower_expr_borrowed(&mut self, expr: ast::AstExpr) -> Operand {
        let node_id = expr.node_id();
        let (value, own) = self.lower_expr(expr);
        self.as_borrowed(
            value,
            own,
            &self
                .cx
                .body
                .node_ty(node_id)
                .unwrap_bug("Expect expr to have type."),
        )
    }

    pub(super) fn lower_expr_owned(&mut self, expr: ast::AstExpr) -> Operand {
        let node_id = expr.node_id();
        let (value, own) = self.lower_expr(expr);
        self.as_owned(
            value,
            own,
            &self
                .cx
                .body
                .node_ty(node_id)
                .unwrap_bug("Expect expr to have type."),
        )
    }

    pub(super) fn lower_field_access(&mut self, field_access: ast::AstFieldAccessExpr) -> Operand {
        let base_ty = self
            .cx
            .body
            .node_ty(field_access.base.node_id())
            .unwrap_or_else(|| bug!("a field access base has no type"));
        let TyKind::Struct(struct_id) = *base_ty.kind() else {
            bug!("a field access base is not a struct");
        };

        let base = self.lower_expr_borrowed(*field_access.base);
        self.lower_field_value_access(base, field_access.field.text, struct_id)
    }

    pub(super) fn lower_struct_init(
        &mut self,
        struct_init: ast::AstStructInitExpr,
    ) -> (Operand, Ownership) {
        let Some(Res::Struct(struct_id)) = self.cx.body.node_res(struct_init.path.node_id()) else {
            bug!("a struct init expression has no resolution (or its resolution is not a struct)");
        };

        let struct_ = self
            .cx
            .tcx
            .structs
            .get_struct(struct_id)
            .unwrap_or_else(|| {
                bug!("unable to resolve struct id for init lowering");
            });

        let layout = calculate_layout(&self.rcx, struct_);
        let tinfo = self.get_cached_tinfo(struct_);
        let tinfo = self
            .builder
            .ins()
            .symbol_value(self.module.target_config().pointer_type(), tinfo);
        let func = self.get_or_cache_function(self.runtime.alloc);
        let call = self.builder.ins().call(func, &[tinfo]);
        let obj = self.builder.inst_results(call)[0];

        let vals = struct_init
            .fields
            .into_iter()
            .map(|field| {
                let idx = struct_
                    .fields
                    .iter()
                    .position(|f| f.name == field.name)
                    .unwrap_or_else(|| {
                        bug!(
                            "unable to find field {} in struct {}",
                            field.name,
                            struct_.name
                        );
                    });
                let v = self.lower_expr_owned(*field.expr);
                (idx, v.expect_scalar("struct fields must be scalar"))
            })
            .collect::<Vec<_>>();

        for (idx, v) in vals {
            let f = &layout.fields[idx];
            self.builder
                .ins()
                .store(MemFlagsData::trusted(), v, obj, f.offset as i32);
        }

        Operand::Scalar(obj).owned()
    }

    pub(super) fn lower_path(&mut self, path_expr: ast::AstPathExpr) -> Operand {
        let res = self
            .cx
            .body
            .node_res(path_expr.node_id)
            .unwrap_or_else(|| bug!("a path expression has no resolution"));
        self.lower_value_res(res)
    }

    pub(super) fn lower_implicit_path(
        &mut self,
        implicit_path_expr: ast::AstImplicitPathExpr,
    ) -> Operand {
        let res = self
            .cx
            .body
            .node_res(implicit_path_expr.node_id)
            .unwrap_or_else(|| bug!("an implicit path expression has no resolution"));
        self.lower_value_res(res)
    }

    /// Lower a res that is a value (for now only enum variants.)
    pub(super) fn lower_value_res(&mut self, res: Res) -> Operand {
        match res {
            Res::Local(..) => {
                Operand::Scalar(self.builder.use_var(self.lookup_local(&res).unwrap_or_else(
                    || bug!("a local variable has no associated cranelift variable"),
                )))
            }
            Res::Param(param_id) => {
                Operand::Scalar(self.builder.block_params(self.entry_block)[param_id.index()])
            }
            Res::Def(def_id) => {
                let callee = self.module.declare_func_in_func(
                    self.cx
                        .def_id_to_function_id(def_id)
                        .unwrap_or_else(|| bug!("a function def_id has no cranelift function id")),
                    self.builder.func,
                );
                let def = self
                    .cx
                    .tcx
                    .defs
                    .def(def_id)
                    .unwrap_or_else(|| bug!("a resolved def_id has no associated def"));
                match def.kind {
                    DefKind::Function(_) => {
                        Operand::Scalar(self.builder.ins().func_addr(func_ty(self.module), callee))
                    }
                }
            }
            Res::ConstraintOld(old_id) => Operand::Scalar(
                self.old_values
                    .get(&old_id)
                    .cloned()
                    .unwrap_or_else(|| bug!("an old value was referenced but never captured"))
                    .expect_scalar("cannot refer to a fallible type as a value"),
            ),
            Res::ConstraintRet => {
                // Really depends on the current block, are we in a success pad?
                if let Some(block) = self.builder.current_block()
                    && matches!(self.block_type, BlockType::PostConstraint)
                {
                    // first block param
                    let block_arg = self.builder.block_params(block)[0];
                    return Operand::Scalar(block_arg);
                }
                // this shouldn't even be possible with typeck..
                unreachable!();
            }
            Res::EnumVariant(enum_variant) => Operand::Scalar(
                self.builder
                    .ins()
                    .iconst(types::I64, enum_variant.as_u32() as i64),
            ),
            Res::Field {
                base,
                field,
                struct_id,
            } => {
                let base = self.lower_value_res(*base);

                self.lower_field_value_access(base, field, struct_id)
            }
            Res::Module(..) => bug!("attempted to resolve a module directly."),
            Res::Enum(..) => bug!("attempted to resolve an enum directly."),
            Res::Struct(..) => bug!("attempted to resolve a struct directly."),
            Res::StructField(..) => bug!("attempted to resolve a struct field directly."),
        }
    }

    pub(super) fn lower_field_value_access(
        &mut self,
        base: Operand,
        field: String,
        struct_id: StructId,
    ) -> Operand {
        let struct_ = self
            .cx
            .tcx
            .structs
            .get_struct(struct_id)
            .unwrap_or_else(|| {
                bug!("unable to resolve struct id for field access lowering");
            });

        let layout = self.get_cached_layout(struct_id);
        let idx = struct_
            .fields
            .iter()
            .position(|f| f.name == field)
            .unwrap_or_else(|| {
                bug!("unable to find field {} in struct {}", field, struct_.name);
            });
        let f = &layout.fields[idx];

        match base {
            Operand::Scalar(base_val) => {
                let v = self.builder.ins().load(
                    f.ty,
                    MemFlagsData::trusted(),
                    base_val,
                    f.offset as i32,
                );

                Operand::Scalar(v)
            }
            _ => bug!("field access base must be a scalar"),
        }
    }

    pub(super) fn lower_ident(&mut self, ident: ast::AstIdent) -> Operand {
        let res = self
            .cx
            .body
            .node_res(ident.node_id)
            .unwrap_or_else(|| bug!("an identifier has no resolution"));
        self.lower_value_res(res)
    }

    pub(super) fn lower_unary_expr(&mut self, unary_expr: ast::AstUnaryExpr) -> Value {
        let x = self
            .lower_expr_borrowed(*unary_expr.expr)
            .expect_scalar("cannot perform comparisons or manipulations on a fallible type");
        let res_ty = self
            .cx
            .body
            .node_ty(unary_expr.node_id)
            .unwrap_or_else(|| bug!("a unary expression has no type"));
        match unary_expr.operator {
            ast::AstUnaryOperator::Negate => match res_ty.kind() {
                TyKind::Int => self.builder.ins().ineg(x),
                TyKind::Float => self.builder.ins().fneg(x),
                TyKind::Void
                | TyKind::Never
                | TyKind::Bool
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..) => {
                    bug!(
                        "tried to negate a non-numeric type at codegen (typeck failed to check it)"
                    )
                }
            },
            ast::AstUnaryOperator::Not => match res_ty.kind() {
                TyKind::Bool => {
                    let one = self.builder.ins().iconst(
                        self.rcx
                            .repr_of(&self.cx.tcx.bool_ty())
                            .expect_scalar("bool is scalar"),
                        1,
                    );
                    self.builder.ins().bxor(x, one)
                }
                TyKind::Int | TyKind::Float => self.builder.ins().bnot(x),
                TyKind::Void
                | TyKind::Never
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..) => {
                    bug!(
                        "tried to do logical or bitwise not on type at codegen (typeck failed to check it)"
                    )
                }
            },
        }
    }

    pub(super) fn lower_bin_expr(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Binary(bin_expr) = expr.kind else {
            unreachable!()
        };
        let res_ty = self
            .cx
            .body
            .node_ty(bin_expr.node_id)
            .unwrap_or_else(|| bug!("a binary expression has no type"));
        self.lower_cond_indirect(*bin_expr.left, *bin_expr.right, res_ty, bin_expr.operator)
    }

    pub(super) fn lower_lit(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Literal(lit) = expr.kind else {
            unreachable!()
        };
        let ty = self
            .cx
            .body
            .node_ty(lit.node_id)
            .unwrap_or_else(|| bug!("a literal has no type"));
        let repr = self.rcx.repr_of(&ty).expect_scalar("literal");
        match ty.kind() {
            TyKind::Int => self.builder.ins().iconst(repr, lit.value.as_int()),
            TyKind::Float => self.builder.ins().f64const(lit.value.as_float()),
            TyKind::Bool => self
                .builder
                .ins()
                .iconst(repr, if lit.value.as_bool() { 1 } else { 0 }),
            TyKind::Enum(..)
            | TyKind::FullFallible(..)
            | TyKind::Func(..)
            | TyKind::Never
            | TyKind::Void
            | TyKind::Struct(..) => {
                bug!("attempted to lower literal of type")
            }
        }
    }
}
