use std::fmt::Display;

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
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

impl Display for AstProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, stmt) in self.statements.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", stmt)?;
        }
        Ok(())
    }
}

pub struct AstStmt {
    // Not used for typing.
    pub node_id: NodeId,
    pub kind: AstStmtKind,
}

impl Display for AstStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

pub enum AstStmtKind {
    Expr(AstExpr),
    Print(AstPrintStmt),
    Decl(AstDecl),
    Assign(AstAssignStmt),
    Block(AstBlockStmt),
    If(AstIfStmt),
}

impl Display for AstStmtKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstStmtKind::Expr(expr) => write!(f, "{};", expr),
            AstStmtKind::Print(print) => write!(f, "{};", print),
            AstStmtKind::Decl(decl) => write!(f, "{};", decl),
            AstStmtKind::Assign(assign) => write!(f, "{};", assign),
            AstStmtKind::Block(block) => write!(f, "{};", block),
            AstStmtKind::If(if_stmt) => write!(f, "{}", if_stmt),
        }
    }
}

pub struct AstBlockStmt {
    pub node_id: NodeId,
    pub stmts: Vec<AstStmt>,
}

impl Display for AstBlockStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stmts.is_empty() {
            return write!(f, "{{}}");
        }
        writeln!(f, "{{")?;
        for stmt in &self.stmts {
            for line in stmt.to_string().lines() {
                writeln!(f, "    {}", line)?;
            }
        }
        write!(f, "}}")
    }
}

pub struct AstIfStmt {
    pub node_id: NodeId,
    pub cond: Box<AstExpr>,
    pub then: Box<AstBlockStmt>,
    pub else_: Option<AstElseBranch>,
}

impl Display for AstIfStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "if {} {}", self.cond, self.then)?;
        if let Some(else_) = &self.else_ {
            write!(f, " else {}", else_)?;
        }
        Ok(())
    }
}

pub enum AstElseBranch {
    Block(AstBlockStmt),
    If(Box<AstIfStmt>),
}

impl Display for AstElseBranch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstElseBranch::Block(block) => write!(f, "{}", block),
            AstElseBranch::If(if_stmt) => write!(f, "{}", if_stmt),
        }
    }
}

pub struct AstAssignStmt {
    pub node_id: NodeId,
    pub name: AstIdent,
    pub expr: Box<AstExpr>,
}

impl Display for AstAssignStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.name, self.expr)
    }
}

pub struct AstExpr {
    pub node_id: NodeId,
    pub kind: AstExprKind,
}

impl Display for AstExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

pub struct AstPrintStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

impl Display for AstPrintStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "print {}", self.expr)
    }
}

pub struct AstDecl {
    pub node_id: NodeId,
    pub kind: AstDeclKind,
}

impl Display for AstDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

pub enum AstDeclKind {
    Let(AstLetDecl),
}

impl Display for AstDeclKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstDeclKind::Let(let_decl) => write!(f, "{}", let_decl),
        }
    }
}

pub struct AstLetDecl {
    pub node_id: NodeId,
    pub name: AstIdent,
    pub expr: Box<AstExpr>,
}

impl Display for AstLetDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "let {} = {}", self.name, self.expr)
    }
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

impl Display for AstExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstExprKind::Binary(binary) => write!(f, "{}", binary),
            AstExprKind::Literal(literal) => write!(f, "{}", literal),
            AstExprKind::Ident(ident) => write!(f, "{}", ident),
        }
    }
}

pub struct AstBinaryExpr {
    pub left: Box<AstExpr>,
    pub right: Box<AstExpr>,
    pub operator: AstBinaryOperator,
}

impl Display for AstBinaryExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({} {} {})", self.left, self.operator, self.right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

impl Display for AstBinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op = match self {
            AstBinaryOperator::Add => "+",
            AstBinaryOperator::Sub => "-",
            AstBinaryOperator::Mul => "*",
            AstBinaryOperator::Div => "/",
            AstBinaryOperator::Eq => "==",
            AstBinaryOperator::Ne => "!=",
            AstBinaryOperator::Lt => "<",
            AstBinaryOperator::Gt => ">",
            AstBinaryOperator::Le => "<=",
            AstBinaryOperator::Ge => ">=",
        };
        write!(f, "{}", op)
    }
}

pub struct AstLiteral {
    pub value: AstLiteralKind,
}

impl Display for AstLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

pub enum AstLiteralKind {
    Int(i64),
    Float(f64),
}

impl Display for AstLiteralKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstLiteralKind::Int(value) => write!(f, "{}", value),
            AstLiteralKind::Float(value) => write!(f, "{:?}", value),
        }
    }
}
