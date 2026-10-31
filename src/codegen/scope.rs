//! Scope frames and the cleanup chains run when leaving them.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    pub(super) fn frame_locals(&self, scope: ScopeId) -> Vec<LocalId> {
        self.cx
            .body
            .scope_locals
            .get(&scope)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Returns a list of locals to declare for the given frame ahead of time.
    pub(super) fn open_frame(&mut self, scope: ScopeId) -> Vec<LocalId> {
        self.scope_frames
            .entry(scope)
            .or_insert_with(|| ScopeFrame {
                scope,
                parent: self.current_scope,
                links: [None; ExitKind::COUNT],
            });

        self.current_scope = Some(scope);
        self.frame_locals(scope)
    }

    /// You are responsible for releasing any resources later before calling this.
    pub(super) fn close_frame(&mut self, scope: ScopeId) {
        let (parent, links) = {
            let Some(frame) = self.scope_frames.get(&scope) else {
                return;
            };
            let links: Vec<(ExitKind, Block)> = [
                ExitKind::Continue,
                ExitKind::Break,
                ExitKind::Return,
                ExitKind::Error,
                ExitKind::Fallthrough,
            ]
            .into_iter()
            .filter_map(|k| frame.links[k.idx()].map(|b| (k, b)))
            .collect();
            (frame.parent, links)
        };

        let cont = if links.is_empty() {
            None
        } else {
            match self.builder.current_block() {
                Some(b) if !self.is_block_terminated(b) => {
                    let c = self.builder.create_block();
                    self.builder.ins().jump(c, &[]);
                    Some(c)
                }
                _ => None,
            }
        };

        for (kind, block) in &links {
            let target = self.locate_cleanup_target(scope, *kind);
            self.builder.switch_to_block(*block);
            // read the exit's params before releasing: use_var in an unsealed
            // block appends params of its own, which the target doesn't take.
            let args: Vec<BlockArg> = self
                .builder
                .block_params(*block)
                .iter()
                .map(|v| BlockArg::Value(*v))
                .collect();
            self.emit_frame_locals_release(scope);

            self.builder.ins().jump(target, &args);
            // no restore — each link is filled before the next switch
        }

        for (_, block) in &links {
            self.builder.seal_block(*block);
        }

        if let Some(c) = cont {
            self.builder.seal_block(c);
            self.builder.switch_to_block(c);
        }

        self.scope_frames.remove(&scope);
        self.current_scope = parent;
    }

    /// Cranelift rep of the expected param type of a cleanup block. This is
    /// future proofed to handle errors (though there's no way to natively throw
    /// errors yet.)
    pub(super) fn chain_repr(&self, kind: ExitKind) -> Repr {
        match kind {
            ExitKind::Return => self.rcx.success_repr_of(&self.cx.body.return_ty),
            ExitKind::Error => self
                .rcx
                .repr_of(&self.cx.body.throws_ty.clone().unwrap_or_else(|| {
                    bug!("attempted to exit scope as an error despite throws_ty not existing")
                })),
            ExitKind::Continue | ExitKind::Break | ExitKind::Fallthrough => Repr::Empty,
        }
    }

    pub(super) fn cleanup_block(&mut self, scope_id: ScopeId, kind: ExitKind) -> Block {
        if let Some(b) = self.frame(scope_id).links[kind.idx()] {
            return b;
        }

        let block = self.builder.create_block();
        for ty in self.chain_repr(kind).types() {
            self.builder.append_block_param(block, ty);
        }

        self.frame_mut(scope_id).links[kind.idx()] = Some(block);

        block
    }

    pub(super) fn locate_cleanup_target(&mut self, scope_id: ScopeId, kind: ExitKind) -> Block {
        let parent = self.frame(scope_id).parent;
        let innermost_loop = self.loops.last();
        let innermost_switch = self.switches.last();
        match kind {
            ExitKind::Continue if innermost_loop.is_some_and(|l| l.scope == scope_id) => {
                innermost_loop
                    .unwrap_or_else(|| bug!("innermost loop was checked to be some, but was none"))
                    .header
            }
            ExitKind::Break if innermost_loop.is_some_and(|l| l.scope == scope_id) => {
                innermost_loop
                    .unwrap_or_else(|| bug!("innermost loop was checked to be some, but was none"))
                    .exit
            }
            ExitKind::Fallthrough if innermost_switch.is_some_and(|s| s.scope == scope_id) => {
                innermost_switch
                    .unwrap_or_else(|| {
                        bug!("innermost switch was checked to be some, but was none")
                    })
                    .upcoming_block
                    .unwrap_or_else(|| {
                        bug!("tried to fallthrough with no next block, which typeck should reject")
                    })
            }
            ExitKind::Continue | ExitKind::Break | ExitKind::Fallthrough => match parent {
                Some(parent) => self.cleanup_block(parent, kind),
                None => unreachable!("continue/break outside of loop is rejected by typeck"),
            },
            ExitKind::Return | ExitKind::Error => match parent {
                Some(p) => self.cleanup_block(p, kind),
                None => match kind {
                    ExitKind::Return => self.success_landing_pad,
                    ExitKind::Error => self.failure_landing_pad,
                    _ => unreachable!(),
                },
            },
        }
    }

    pub(super) fn emit_frame_locals_declare(&mut self, scope_id: ScopeId) {
        for local_id in self.frame_locals(scope_id).into_iter() {
            let ty = self
                .cx
                .body
                .local_ty(local_id)
                .unwrap_or_else(|| bug!("a frame local has no type"));
            let variable = self
                .builder
                .declare_var(self.rcx.repr_of(&ty).expect_scalar("local variable"));
            self.insert(Res::Local(local_id), variable);

            if ty.is_ref_counted() {
                let value = self
                    .builder
                    .ins()
                    .iconst(self.module.target_config().pointer_type(), 0);
                self.builder.def_var(variable, value);
            }
        }
    }

    pub(super) fn emit_frame_locals_release(&mut self, scope_id: ScopeId) {
        for local_id in self.frame_locals(scope_id).into_iter() {
            let ty = self
                .cx
                .body
                .local_ty(local_id)
                .unwrap_or_else(|| bug!("a frame local has no type"));

            if ty.is_ref_counted() {
                let var = self.locals[&local_id];
                let val = self.builder.use_var(var);
                self.emit_release(val);
            }
        }
    }

    pub(super) fn flush_temps(&mut self) {
        let temps = mem::take(&mut self.temps);
        for temp in temps {
            self.emit_release(temp);
        }
    }
}
