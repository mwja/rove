use std::fmt::Display;

indexable_id!(pub NodeId);

pub struct AstProgram {
    pub defs: Vec<AstDef>,
}

#[derive(Debug)]
pub enum AstType {
    Int,
    Float,
    Void,
}

impl Display for AstType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AstType::Int => "int",
                AstType::Float => "float",
                AstType::Void => "void",
            }
        )
    }
}

#[derive(Debug)]
pub enum AstDef {
    Function(AstFunctionDef),
}

impl AstDef {
    pub fn node_id(&self) -> NodeId {
        match self {
            AstDef::Function(func) => func.node_id,
        }
    }
}

impl Display for AstDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstDef::Function(func) => write!(f, "{}", func),
        }
    }
}

#[derive(Debug)]
pub struct AstFunctionDef {
    pub node_id: NodeId,
    pub name: String,
    pub args: Vec<AstArgDef>,
    pub return_node_id: Option<NodeId>,
    pub return_ty: AstType,
    pub body: AstBlockStmt,
    pub is_main: bool,
}

impl Display for AstFunctionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "func {}({}) -> {}",
            self.name,
            self.args
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.return_ty
        )?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct AstArgDef {
    pub node_id: NodeId,
    pub name: String,
    pub ty: AstType,
}

impl Display for AstArgDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.ty)
    }
}

impl Display for AstProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, def) in self.defs.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", def)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct AstStmt {
    pub kind: AstStmtKind,
}

impl AstStmt {
    pub fn inner_node_id(&self) -> NodeId {
        match &self.kind {
            AstStmtKind::Expr(expr) => expr.node_id(),
            AstStmtKind::Print(print) => print.node_id,
            AstStmtKind::Decl(decl) => decl.inner_node_id(),
            AstStmtKind::Assign(assign) => assign.node_id,
            AstStmtKind::Block(block) => block.node_id,
            AstStmtKind::If(if_stmt) => if_stmt.node_id,
            AstStmtKind::Return(return_stmt) => return_stmt.node_id,
            AstStmtKind::Loop(loop_stmt) => loop_stmt.node_id,
            AstStmtKind::While(while_stmt) => while_stmt.node_id,
            AstStmtKind::Break(break_stmt) => break_stmt.node_id,
            AstStmtKind::Continue(continue_stmt) => continue_stmt.node_id,
        }
    }

    pub fn is_loop(&self) -> bool {
        matches!(self.kind, AstStmtKind::Loop(_) | AstStmtKind::While(_))
    }
}

impl Display for AstStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(Debug)]
pub enum AstStmtKind {
    Expr(AstExpr),
    Print(AstPrintStmt),
    Decl(AstDecl),
    Assign(AstAssignStmt),
    Block(AstBlockStmt),
    If(AstIfStmt),
    Loop(AstLoopStmt),
    While(AstWhileStmt),
    Break(AstBreakStmt),
    Continue(AstContinueStmt),
    Return(AstReturnStmt),
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
            AstStmtKind::Return(return_stmt) => write!(f, "{};", return_stmt),
            AstStmtKind::Loop(loop_stmt) => write!(f, "{};", loop_stmt),
            AstStmtKind::While(while_stmt) => write!(f, "{};", while_stmt),
            AstStmtKind::Break(break_stmt) => write!(f, "{};", break_stmt),
            AstStmtKind::Continue(continue_stmt) => write!(f, "{};", continue_stmt),
        }
    }
}

#[derive(Debug)]
pub struct AstLoopStmt {
    pub node_id: NodeId,
    pub body: AstBlockStmt,
}

impl Display for AstLoopStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loop {{ {} }}", self.body)
    }
}

#[derive(Debug)]
pub struct AstWhileStmt {
    pub node_id: NodeId,
    pub cond: Box<AstExpr>,
    pub body: AstBlockStmt,
}

impl Display for AstWhileStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "while {} {{ {} }}", self.cond, self.body)
    }
}

#[derive(Debug)]
pub struct AstBreakStmt {
    pub node_id: NodeId,
}

impl Display for AstBreakStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "break")
    }
}

#[derive(Debug)]
pub struct AstContinueStmt {
    pub node_id: NodeId,
}

impl Display for AstContinueStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "continue")
    }
}

#[derive(Debug)]
pub struct AstReturnStmt {
    /// Only for diagnostics, do NOT type this
    pub node_id: NodeId,
    pub expr: Option<AstExpr>,
}

impl Display for AstReturnStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "return")?;
        if let Some(expr) = &self.expr {
            write!(f, " {}", expr)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct AstExpr {
    pub kind: AstExprKind,
}

impl AstExpr {
    /// Convenience method to get the node id of this expression regardless
    /// of what type of expression it is.
    pub fn node_id(&self) -> NodeId {
        match &self.kind {
            AstExprKind::Literal(lit) => lit.node_id,
            AstExprKind::Binary(bin_expr) => bin_expr.node_id,
            AstExprKind::Ident(ident) => ident.node_id,
            AstExprKind::Call(call) => call.node_id,
        }
    }
}

impl Display for AstExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(Debug)]
pub struct AstPrintStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

impl Display for AstPrintStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "print {}", self.expr)
    }
}

#[derive(Debug)]
pub struct AstDecl {
    pub kind: AstDeclKind,
}

impl AstDecl {
    pub fn inner_node_id(&self) -> NodeId {
        match &self.kind {
            AstDeclKind::Let(let_decl) => let_decl.inner_node_id(),
        }
    }
}

impl Display for AstDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct AstLetDecl {
    pub name: AstIdent,
    pub expr: Box<AstExpr>,
}

impl AstLetDecl {
    pub fn inner_node_id(&self) -> NodeId {
        self.name.node_id
    }
}

impl Display for AstLetDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "let {} = {}", self.name, self.expr)
    }
}

#[derive(Debug)]
pub struct AstIdent {
    pub node_id: NodeId,
    pub text: String,
}

impl Display for AstIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

#[derive(Debug)]
pub enum AstExprKind {
    Binary(AstBinaryExpr),
    Literal(AstLiteral),
    Ident(AstIdent),
    Call(AstCallExpr),
}

#[derive(Debug)]
pub struct AstCallExpr {
    pub node_id: NodeId,
    pub callee: Box<AstExpr>,
    pub args: Vec<Box<AstExpr>>,
}

impl Display for AstCallExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}({})",
            self.callee,
            self.args
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
impl Display for AstExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstExprKind::Binary(binary) => write!(f, "{}", binary),
            AstExprKind::Literal(literal) => write!(f, "{}", literal),
            AstExprKind::Ident(ident) => write!(f, "{}", ident),
            AstExprKind::Call(call) => write!(f, "{}", call),
        }
    }
}

#[derive(Debug)]
pub struct AstBinaryExpr {
    pub node_id: NodeId,
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

#[derive(Debug)]
pub struct AstLiteral {
    pub node_id: NodeId,
    pub value: AstLiteralKind,
}

impl Display for AstLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Debug)]
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
