use std::collections::HashMap;

use thiserror::Error;

use crate::{
    ast::{self, NodeId},
    sourcemap::{DiagnoseWith, SpanRecorder, report::Diagnostic},
    ty::{
        Ty, TyCtxt,
        res::{ModuleContext, TyResolutionError, resolve_type},
    },
};

indexable_id!(pub StructId);

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub mutable: bool,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub id: StructId,
    pub name: String,
    pub fields: Vec<StructField>,
}

impl Struct {
    pub fn get_field(&self, name: &str) -> Option<&StructField> {
        self.fields.iter().find(|field| field.name == name)
    }

    pub fn get_field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|field| field.name == name)
    }
}

#[derive(Debug, Default)]
pub struct Structs {
    next_id: usize,
    node_id_to_struct_id: HashMap<NodeId, StructId>,
    structs: Vec<Struct>,
}

impl_next_id!(Structs.next_id -> StructId);

impl Structs {
    fn declare_struct(&mut self, node_id: NodeId) -> StructId {
        let id = self.next_id();

        self.node_id_to_struct_id.insert(node_id, id);

        id
    }

    fn define_struct(&mut self, id: StructId, name: String, fields: Vec<StructField>) {
        let struct_ = Struct { id, name, fields };
        self.structs.push(struct_);
    }

    pub fn resolve_node_id(&self, node_id: NodeId) -> Option<StructId> {
        self.node_id_to_struct_id.get(&node_id).copied()
    }

    pub fn get_struct(&self, id: StructId) -> Option<&Struct> {
        self.structs.get(id.index())
    }
}

#[derive(Debug, Error)]
pub enum StructError {
    // I _will_ allow this, but later when weak references are added with ARC
    // and nullability is introduced as a concept to Rove.
    #[error("struct cannot be sized as it references itself directly")]
    RecursiveStruct(NodeId, NodeId),
    #[error(transparent)]
    TyResolutionError(#[from] TyResolutionError),
}

impl StructError {
    pub fn as_code(&self) -> usize {
        match self {
            StructError::RecursiveStruct(_, _) => 5001,
            StructError::TyResolutionError(err) => err.as_code(),
        }
    }
}

impl DiagnoseWith<NodeId> for StructError {
    fn diagnose_with(&self, recorder: &mut SpanRecorder<NodeId>) -> Vec<Diagnostic> {
        use StructError::*;

        let code = Some(self.as_code());
        match self {
            RecursiveStruct(node_id, enclosing_struct_node_id) => {
                vec![
                    Diagnostic::new(self.to_string())
                        .with_code(code)
                        .with_label_from(recorder, node_id, "this field is recursive")
                        .with_label_from(recorder, enclosing_struct_node_id, "inside this struct"),
                ]
            }
            TyResolutionError(ty_res_error) => ty_res_error.diagnose_with(recorder),
        }
    }
}

pub fn declare(ast: &ast::AstModule, structs: &mut Structs) {
    for struct_def in &ast.structs {
        structs.declare_struct(struct_def.node_id);
    }

    for module in &ast.mods {
        declare(module, structs);
    }
}

pub fn define(tcx: &mut TyCtxt, ast: &ast::AstModule) -> Result<(), Vec<StructError>> {
    let mut errors = Vec::new();
    for struct_def in &ast.structs {
        let id = tcx.structs.resolve_node_id(struct_def.node_id).unwrap();
        let name = struct_def.name.clone();

        let mut fields = Vec::new();
        for field in &struct_def.fields {
            let ty = match resolve_type(
                tcx,
                tcx.mcx
                    .get_module_id(ast.node_id)
                    .unwrap_or_else(|| bug!("can't find module for node")),
                &field.ty,
            ) {
                Ok(ty) => ty,
                Err(e) => {
                    errors.push(e.into());
                    continue;
                }
            };

            fields.push(StructField {
                name: field.name.clone(),
                mutable: field.mutable,
                ty,
            });
        }

        tcx.structs.define_struct(id, name, fields);
    }

    for module in &ast.mods {
        match define(tcx, module) {
            Ok(_) => {}
            Err(mut module_errors) => {
                errors.append(&mut module_errors);
            }
        }
    }

    Ok(())
}
