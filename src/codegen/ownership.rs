//! Stores models of ownership tracked during codegen

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Ownership {
    Borrowed,
    Owned,
}
