pub struct NodeId(usize);

impl NodeId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

pub struct AstProgram {
    pub statements: Vec<AstStmt>,
}

pub struct AstStmt {
    pub node_id: NodeId,
    pub kind: AstStmtKind,
}

pub enum AstStmtKind {
    Expr(AstExpr),
    Print(AstPrintStmt),
}

pub struct AstExpr {
    pub node_id: NodeId,
    pub kind: AstExprKind,
}

pub struct AstPrintStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

pub enum AstExprKind {
    Binary(AstBinaryExpr),
    Literal(AstLiteral),
}

pub struct AstBinaryExpr {
    pub node_id: NodeId,
    pub left: Box<AstExpr>,
    pub right: Box<AstExpr>,
    pub operator: AstBinaryOperator,
}

pub enum AstBinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
}

pub struct AstLiteral {
    pub node_id: NodeId,
    pub value: AstLiteralKind,
}

pub enum AstLiteralKind {
    Int(i64),
}
