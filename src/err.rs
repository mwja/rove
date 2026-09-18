//! Handles errors and error sets in Rove. They are used within the type system.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Display,
};

use crate::{
    ast::{self, AstErrorSet, NodeId},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::TyCtxt,
};
#[derive(Debug)]
pub struct ErrorSet {
    next_error_id: usize,
    set_id: ErrorSetId,
    errors: HashMap<String, RawErrorId>,
}

indexable_id!(pub RawErrorId);
impl_next_id!(ErrorSet.next_error_id -> RawErrorId);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorId(ErrorSetId, RawErrorId);

impl Display for ErrorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} in error set {}", self.1.index(), self.0.index())
    }
}

// errerror, not exactly the most readable but okay.
#[derive(Debug, thiserror::Error)]
pub enum ErrError {
    #[error("duplicate definitions of an error set: {0}")]
    DuplicateSet(String, NodeId, NodeId),
    #[error("duplicate definitions inside a set: {0}")]
    DuplicateInside(String, NodeId),
}

impl ErrError {
    pub fn as_code(&self) -> usize {
        match self {
            ErrError::DuplicateSet(_, _, _) => 3001,
            ErrError::DuplicateInside(_, _) => 3002,
        }
    }
}

impl DiagnoseWith<NodeId> for ErrError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        use ErrError::*;
        let message = self.to_string();
        match self {
            DuplicateSet(name, old_node_id, new_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(Some(self.as_code()))
                        .with_label_from(
                            recorder,
                            old_node_id,
                            format!("previous definition of error set `{name}` was here"),
                        )
                        .with_label_from(recorder, new_node_id, "new definition here"),
                ]
            }
            DuplicateInside(name, new_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(Some(self.as_code()))
                        .with_label_from(
                            recorder,
                            new_node_id,
                            format!("duplicate definition of error `{name}` is here"),
                        ),
                ]
            }
        }
    }
}

impl ErrorSet {
    pub fn new(set_id: ErrorSetId) -> Self {
        Self {
            next_error_id: 0,
            set_id,
            errors: HashMap::new(),
        }
    }

    pub fn new_with(set_id: ErrorSetId, names: impl IntoIterator<Item = String>) -> Self {
        let errors: HashMap<_, _> = names
            .into_iter()
            .enumerate()
            .map(|(v, k)| (k, RawErrorId(v)))
            .collect();

        Self {
            next_error_id: errors.len(),
            set_id,
            errors,
        }
    }

    fn error_id(&self, raw: RawErrorId) -> ErrorId {
        ErrorId(self.set_id, raw)
    }

    pub fn get_id(&self, name: &str) -> Option<ErrorId> {
        self.errors.get(name).map(|raw| self.error_id(*raw))
    }

    pub fn get_name(&self, id: ErrorId) -> Option<&String> {
        if id.0 != self.set_id {
            return None;
        };
        self.errors
            .iter()
            .find_map(|(k, v)| if v == &id.1 { Some(k) } else { None })
    }

    pub fn intern(
        &mut self,
        name: impl Into<String>,
        node_id: NodeId,
    ) -> Result<ErrorId, ErrError> {
        let name = name.into();
        if self.get_id(&name).is_some() {
            return Err(ErrError::DuplicateInside(name, node_id));
        }

        let id = self.next_id();
        self.errors.insert(name, id);

        Ok(self.error_id(id))
    }

    /// Returns a new error set, with both sets names but with pre-configured
    /// IDs.
    ///
    /// This explicitly merges shared names; if `self` has `MyError`, and
    /// `other` has `MyError`, both will refer to the same error (and same i32
    /// tag). The first item is alwys picked (see [Vec::dedup])
    pub fn merge_as(&self, other: &ErrorSet, set_id: ErrorSetId) -> ErrorSet {
        let mut errors: Vec<_> = self
            .errors
            .keys()
            .chain(other.errors.keys())
            .cloned()
            .collect();

        errors.dedup();

        ErrorSet::new_with(set_id, errors)
    }
}

indexable_id!(pub ErrorSetId);
impl_next_id!(ErrorSetCtxt.next_error_set_id -> ErrorSetId);

#[derive(Debug)]
pub struct ErrorSetCtxt {
    next_error_set_id: usize,
    error_sets: Vec<ErrorSet>,
    name_to_error_set_id: HashMap<String, ErrorSetId>,
    error_set_to_node_id: HashMap<ErrorSetId, NodeId>,
}

impl ErrorSetCtxt {
    pub fn new() -> Self {
        Self {
            next_error_set_id: 0,
            error_sets: Vec::new(),
            name_to_error_set_id: HashMap::new(),
            error_set_to_node_id: HashMap::new(),
        }
    }

    pub fn create_set(&mut self, name: String, node_id: NodeId) -> Result<ErrorSetId, ErrError> {
        let id = self.next_id();

        // attempt to insert but fail if duplicate.
        if let Some(old_id) = self.name_to_error_set_id.insert(name.clone(), id) {
            // duplicate
            return Err(ErrError::DuplicateSet(
                name,
                *self.error_set_to_node_id.get(&old_id).unwrap(),
                node_id,
            ));
        }

        self.error_set_to_node_id.insert(id, node_id);
        self.error_sets.push(ErrorSet::new(id));

        Ok(id)
    }

    pub fn get_set(&self, error_set_id: ErrorSetId) -> &ErrorSet {
        &self.error_sets[error_set_id.index()]
    }

    pub fn get_set_mut(&mut self, error_set_id: ErrorSetId) -> &mut ErrorSet {
        &mut self.error_sets[error_set_id.index()]
    }

    pub fn create_merged_set(&mut self, left: ErrorSetId, right: ErrorSetId) -> ErrorSetId {
        let set_id = self.next_id();
        self.error_sets
            .push(self.get_set(left).merge_as(self.get_set(right), set_id));

        set_id
    }

    pub fn finish(self) -> ErrorSets {
        ErrorSets {
            sets: self.error_sets,
            name_to_error_set_id: self.name_to_error_set_id,
            error_set_to_node_id: self.error_set_to_node_id,
        }
    }
}

#[derive(Debug, Default)]
pub struct ErrorSets {
    pub sets: Vec<ErrorSet>,
    pub name_to_error_set_id: HashMap<String, ErrorSetId>,
    pub error_set_to_node_id: HashMap<ErrorSetId, NodeId>,
}

impl ErrorSets {
    pub fn get_set(&self, error_set_id: ErrorSetId) -> &ErrorSet {
        &self.sets[error_set_id.index()]
    }

    pub fn get_set_by_name(&self, name: &str) -> Option<&ErrorSet> {
        self.name_to_error_set_id
            .get(name)
            .map(|id| self.get_set(*id))
    }

    pub fn get_set_id_by_name(&self, name: &str) -> Option<ErrorSetId> {
        self.name_to_error_set_id.get(name).copied()
    }
}

/// Like the def pass, but for errors
pub fn resolve(tcx: &mut TyCtxt, program: &ast::AstProgram) -> Result<(), Vec<ErrError>> {
    let mut ecx = ErrorSetCtxt::new();

    resolve_names(&mut ecx, program)?;
    // when we have combined sets.
    tcx.errs = ecx.finish();
    Ok(())
}

pub fn resolve_names(
    ecx: &mut ErrorSetCtxt,
    program: &ast::AstProgram,
) -> Result<(), Vec<ErrError>> {
    let mut errors = Vec::new();
    for err in program.error_sets.iter() {
        match ecx.create_set(err.name.clone(), err.node_id) {
            Ok(set_id) => {
                let set = ecx.get_set_mut(set_id);
                for err in err.errors.iter() {
                    if let Err(e) = set.intern(err.text.clone(), err.node_id) {
                        errors.push(e);
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(())
    }
}
