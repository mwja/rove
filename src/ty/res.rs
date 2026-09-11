indexable_id!(pub LocalId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
    Local(LocalId),
    Err,
}
