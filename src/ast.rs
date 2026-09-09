use std::fmt::Display;

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
    Decl(AstDecl),
    Assign(AstAssignStmt),
    Block(AstBlockStmt),
    If(AstIfStmt),
}

pub struct AstBlockStmt {
    pub node_id: NodeId,
    pub stmts: Vec<AstStmt>,
}

pub struct AstIfStmt {
    pub node_id: NodeId,
    pub cond: Box<AstExpr>,
    pub then: Box<AstBlockStmt>,
    pub else_: Option<AstElseBranch>,
}

pub enum AstElseBranch {
    Block(AstBlockStmt),
    If(Box<AstIfStmt>),
}

pub struct AstAssignStmt {
    pub node_id: NodeId,
    pub name: AstIdent,
    pub expr: Box<AstExpr>,
}

pub struct AstExpr {
    pub node_id: NodeId,
    pub kind: AstExprKind,
}

pub struct AstPrintStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

pub struct AstDecl {
    pub node_id: NodeId,
    pub kind: AstDeclKind,
}

pub enum AstDeclKind {
    Let(AstLetDecl),
}

pub struct AstLetDecl {
    pub node_id: NodeId,
    pub name: AstIdent,
    pub expr: Box<AstExpr>,
}

pub struct AstIdent {
    pub text: String,
}

impl Display for AstIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

pub enum AstExprKind {
    Binary(AstBinaryExpr),
    Literal(AstLiteral),
    Ident(AstIdent),
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
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

pub struct AstLiteral {
    pub node_id: NodeId,
    pub value: AstLiteralKind,
}

pub enum AstLiteralKind {
    Int(i64),
}
