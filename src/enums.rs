use std::{collections::HashMap, fmt::Display};

use crate::{
    ast::{self, NodeId},
    sourcemap::{DiagnoseWith, SpanRecorder, report::Diagnostic},
};

indexable_id!(pub EnumId);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumIndex(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumVariant(EnumId, EnumIndex);

impl Display for EnumVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>::<variant {}>", self.0.0, self.1.0)
    }
}

impl EnumVariant {
    pub fn new(enum_id: EnumId, index: EnumIndex) -> Self {
        Self(enum_id, index)
    }

    pub fn enum_id(&self) -> EnumId {
        self.0
    }

    pub fn is_same_enum(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn index(&self) -> EnumIndex {
        self.1
    }

    pub fn as_u32(&self) -> u32 {
        self.1.0
    }
}

#[derive(Debug)]
pub struct Enum {
    pub id: EnumId,
    pub name: String,
    pub variants: Vec<String>,
}

impl Enum {
    pub fn new(id: EnumId, name: String, variants: Vec<String>) -> Self {
        Self { id, name, variants }
    }

    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    pub fn variant_name(&self, index: EnumIndex) -> Option<&str> {
        self.variants.get(index.0 as usize).map(|s| s.as_str())
    }

    pub fn variant_index(&self, name: &str) -> Option<EnumIndex> {
        self.variants
            .iter()
            .position(|s| s == name)
            .map(|i| EnumIndex(i as u32))
    }

    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        self.variants
            .iter()
            .enumerate()
            .map(move |(i, _)| EnumVariant::new(self.id, EnumIndex(i as u32)))
    }

    pub fn get_variant_by_name(&self, name: &str) -> Option<EnumVariant> {
        self.variant_index(name)
            .map(|index| EnumVariant::new(self.id, index))
    }

    pub fn get_variant_by_name_as_u32(&self, name: &str) -> Option<u32> {
        self.get_variant_by_name(name)
            .map(|variant| variant.as_u32())
    }

    pub fn name_of_variant(&self, variant: EnumVariant) -> Option<&str> {
        if variant.enum_id() != self.id {
            return None;
        }
        self.variant_name(variant.index())
    }
}

#[derive(Debug, Default)]
pub struct Enums {
    next_id: usize,
    node_id_to_enum_id: HashMap<NodeId, EnumId>,
    name_to_enum_id: HashMap<String, EnumId>,
    enums: Vec<Enum>,
}

impl Enums {
    fn new() -> Self {
        Self {
            next_id: 0,
            node_id_to_enum_id: HashMap::new(),
            name_to_enum_id: HashMap::new(),
            enums: Vec::new(),
        }
    }

    fn add_enum(&mut self, node_id: NodeId, name: String, variants: Vec<String>) -> EnumId {
        let id = self.next_id();
        self.name_to_enum_id.insert(name.clone(), id);
        let enum_def = Enum::new(id, name, variants);
        self.node_id_to_enum_id.insert(node_id, id);
        self.enums.push(enum_def);
        id
    }

    pub fn resolve_name(&self, name: &str) -> Option<EnumId> {
        self.name_to_enum_id.get(name).copied()
    }

    pub fn get_enum(&self, id: EnumId) -> Option<&Enum> {
        self.enums.get(id.0 as usize)
    }

    pub fn get_enum_by_name(&self, name: &str) -> Option<&Enum> {
        self.name_to_enum_id
            .get(name)
            .and_then(|id| self.get_enum(*id))
    }
}

impl_next_id!(Enums.next_id -> EnumId);

#[derive(Debug, thiserror::Error)]
pub enum EnumError {
    #[error("duplicate definitions of enum {0}")]
    DuplicateEnum(String, NodeId, NodeId),
    #[error("duplicate variant {0} in enum")]
    DuplicateVariant(String, NodeId, NodeId),
}

impl EnumError {
    fn as_code(&self) -> usize {
        match self {
            EnumError::DuplicateEnum(..) => 4001,
            EnumError::DuplicateVariant(..) => 4002,
        }
    }
}

impl DiagnoseWith<NodeId> for EnumError {
    fn diagnose_with(&self, recorder: &mut SpanRecorder<NodeId>) -> Vec<Diagnostic> {
        let (original, duplicate) = match self {
            EnumError::DuplicateEnum(_, original, duplicate)
            | EnumError::DuplicateVariant(_, original, duplicate) => (original, duplicate),
        };
        vec![
            Diagnostic::new(self.to_string())
                .with_code(Some(self.as_code()))
                .with_label_from(recorder, original, "original definition here")
                .with_label_from(recorder, duplicate, "duplicate definition here"),
        ]
    }
}

pub fn resolve(ast: &ast::AstProgram) -> Result<Enums, Vec<EnumError>> {
    let mut enums = Enums::new();
    let mut errors = Vec::new();
    let mut seen_enums: HashMap<&str, NodeId> = HashMap::new();
    for enum_def in ast.enums.iter() {
        if let Some(original) = seen_enums.insert(&enum_def.name, enum_def.node_id) {
            errors.push(EnumError::DuplicateEnum(
                enum_def.name.clone(),
                original,
                enum_def.node_id,
            ));
            continue;
        }

        let mut seen_variants: HashMap<&str, NodeId> = HashMap::new();
        for variant in enum_def.variants.iter() {
            if let Some(original) = seen_variants.insert(&variant.text, variant.node_id) {
                errors.push(EnumError::DuplicateVariant(
                    variant.text.clone(),
                    original,
                    variant.node_id,
                ));
            }
        }

        let variants = enum_def.variants.iter().map(|v| v.text.clone()).collect();
        enums.add_enum(enum_def.node_id, enum_def.name.clone(), variants);
    }

    if errors.is_empty() {
        Ok(enums)
    } else {
        Err(errors)
    }
}
