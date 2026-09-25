use std::fmt::Display;

indexable_id!(pub NodeId);

pub struct AstProgram {
    pub defs: Vec<AstDef>,
    pub enums: Vec<AstEnumDef>,
}

pub struct AstEnumDef {
    pub node_id: NodeId,
    pub name: String,
    pub variants: Vec<AstIdent>,
}

impl Display for AstEnumDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "enum {} {{ {} }}",
            self.name,
            self.variants
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug)]
pub enum AstType {
    Int,
    Float,
    Void,
    Path(AstPath),
}

impl Display for AstType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AstType::Int => "int".to_owned(),
                AstType::Float => "float".to_owned(),
                AstType::Void => "void".to_owned(),
                AstType::Path(path) => path.to_string(),
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
    pub throws: Option<AstType>,
    pub throws_node_id: Option<NodeId>,
    pub return_ty: AstType,
    pub body: AstBlockStmt,
    pub is_main: bool,
    pub constraints: Vec<AstConstraint>,
}

impl Display for AstFunctionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "func {}({}) {} -> {}",
            self.name,
            self.args
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.constraints
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            // self.throws
            //     .as_ref()
            //     .map(|e| format!("throws {}", e))
            //     .unwrap_or_default(),
            self.return_ty
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum AstConstraint {
    Require(AstRequireConstraint),
    Ensure(AstEnsureConstraint),
    Guard(AstGuardConstraint),
}

impl AstConstraint {
    pub fn node_id(&self) -> NodeId {
        match self {
            AstConstraint::Require(AstRequireConstraint { node_id, .. })
            | AstConstraint::Ensure(AstEnsureConstraint { node_id, .. })
            | AstConstraint::Guard(AstGuardConstraint { node_id, .. }) => *node_id,
        }
    }

    pub fn condition(&self) -> &AstExpr {
        match self {
            AstConstraint::Require(AstRequireConstraint { condition, .. })
            | AstConstraint::Ensure(AstEnsureConstraint { condition, .. })
            | AstConstraint::Guard(AstGuardConstraint { condition, .. }) => condition,
        }
    }

    pub fn tag(&self) -> Option<String> {
        match self {
            AstConstraint::Require(AstRequireConstraint { tag, .. })
            | AstConstraint::Ensure(AstEnsureConstraint { tag, .. }) => tag.clone(),
            AstConstraint::Guard(AstGuardConstraint { .. }) => None,
        }
    }
}

impl Display for AstConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstConstraint::Require(c) => write!(
                f,
                "require! {}: {}",
                c.tag.as_deref().unwrap_or(""),
                c.condition
            ),
            AstConstraint::Ensure(c) => write!(
                f,
                "ensure! {}: {}",
                c.tag.as_deref().unwrap_or(""),
                c.condition
            ),
            AstConstraint::Guard(c) => {
                write!(f, "guard {} if {}", c.error, c.condition)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstRequireConstraint {
    pub node_id: NodeId,
    pub tag: Option<String>,
    pub condition: Box<AstExpr>,
}

#[derive(Debug, Clone)]
pub struct AstEnsureConstraint {
    pub node_id: NodeId,
    pub tag: Option<String>,
    pub condition: Box<AstExpr>,
}

#[derive(Debug, Clone)]
pub struct AstGuardConstraint {
    pub node_id: NodeId,
    pub error: AstExpr,
    pub condition: Box<AstExpr>,
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
            AstStmtKind::Fallthrough(fallthrough_stmt) => fallthrough_stmt.node_id,
            AstStmtKind::ImplicitReturn(expr) => expr.node_id(),
            AstStmtKind::Switch(switch_stmt) => switch_stmt.node_id,
            AstStmtKind::Throw(throw_stmt) => throw_stmt.node_id,
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
    Fallthrough(AstFallthroughStmt),
    Return(AstReturnStmt),
    ImplicitReturn(AstExpr),
    Switch(AstSwitchStmt),
    Throw(AstThrowStmt),
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
            AstStmtKind::Fallthrough(fallthrough_stmt) => write!(f, "{};", fallthrough_stmt),
            AstStmtKind::ImplicitReturn(expr) => write!(f, "{}", expr),
            AstStmtKind::Switch(switch_stmt) => write!(f, "{}", switch_stmt),
            AstStmtKind::Throw(throw_stmt) => write!(f, "{};", throw_stmt),
        }
    }
}

#[derive(Debug)]
pub struct AstThrowStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

impl Display for AstThrowStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "throw {}", self.expr)
    }
}

#[derive(Debug)]
pub struct AstSwitchStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
    pub cases: Vec<AstSwitchCase>,
}

impl AstSwitchStmt {
    pub fn has_else(&self) -> bool {
        self.cases
            .iter()
            .any(|case| matches!(case, AstSwitchCase::Else(_)))
    }

    pub fn else_case(&self) -> Option<&AstSwitchElseCase> {
        self.cases.iter().find_map(|case| {
            if let AstSwitchCase::Else(else_case) = case {
                Some(else_case)
            } else {
                None
            }
        })
    }

    /// Iterates only the real cases.
    pub fn cases(&self) -> impl Iterator<Item = &AstSwitchCaseItem> {
        self.cases.iter().filter_map(|case| {
            if let AstSwitchCase::Case(case_item) = case {
                Some(case_item)
            } else {
                None
            }
        })
    }

    pub fn consume(
        self,
    ) -> (
        Box<AstExpr>,
        Option<AstSwitchElseCase>,
        Vec<AstSwitchCaseItem>,
    ) {
        let mut else_case = None;
        let mut cases = Vec::new();

        for case in self.cases {
            match case {
                AstSwitchCase::Case(case_item) => cases.push(case_item),
                AstSwitchCase::Else(else_case_item) => {
                    if else_case.is_some() {
                        panic!("Multiple else cases in switch statement");
                    }
                    else_case = Some(else_case_item);
                }
            }
        }

        (self.expr, else_case, cases)
    }
}

impl Display for AstSwitchStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "switch {} {{", self.expr)?;
        for case in &self.cases {
            writeln!(f, "    {}", case)?;
        }
        write!(f, "}}")
    }
}

#[derive(Debug)]
pub enum AstSwitchCase {
    Case(AstSwitchCaseItem),
    Else(AstSwitchElseCase),
}

impl AstSwitchCase {
    pub fn node_id(&self) -> NodeId {
        match self {
            AstSwitchCase::Case(case) => case.node_id,
            AstSwitchCase::Else(else_case) => else_case.node_id,
        }
    }

    pub fn body(&self) -> &AstBlockStmt {
        match self {
            AstSwitchCase::Case(case) => &case.body,
            AstSwitchCase::Else(else_case) => &else_case.body,
        }
    }
}

impl Display for AstSwitchCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstSwitchCase::Case(case) => write!(f, "{}", case),
            AstSwitchCase::Else(else_case) => write!(f, "{}", else_case),
        }
    }
}

#[derive(Debug)]
pub struct AstSwitchCaseItem {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
    pub body: AstBlockStmt,
}

impl Display for AstSwitchCaseItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "case {}: {}", self.expr, self.body)
    }
}

#[derive(Debug)]
pub struct AstSwitchElseCase {
    pub node_id: NodeId,
    pub body: AstBlockStmt,
}

impl Display for AstSwitchElseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "else: {}", self.body)
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
pub struct AstFallthroughStmt {
    pub node_id: NodeId,
}

impl Display for AstFallthroughStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fallthrough")
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

#[derive(Debug, Clone)]
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
            AstExprKind::Path(field_access) => field_access.node_id,
            AstExprKind::ImplicitPath(implicit_field_access) => implicit_field_access.node_id, // AstExprKind::ForcedTry(forced_try) => forced_try.node_id,
            AstExprKind::ForcedTry(forced_try) => forced_try.node_id,
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

#[derive(Debug, Clone)]
pub struct AstIdent {
    pub node_id: NodeId,
    pub text: String,
}

impl AstIdent {
    pub fn is_constraint_kw_ret(&self) -> bool {
        self.text == "ret"
    }
}

impl Display for AstIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

#[derive(Debug, Clone)]
pub enum AstExprKind {
    Binary(AstBinaryExpr),
    Literal(AstLiteral),
    Ident(AstIdent),
    Call(AstCallExpr),
    Path(AstPathExpr),
    ImplicitPath(AstImplicitPathExpr),
    ForcedTry(AstForcedTryExpr),
}

#[derive(Debug, Clone)]
pub struct AstPathExpr {
    pub node_id: NodeId,
    pub base: Box<AstPath>,
    pub field: AstIdent,
}

impl Display for AstPathExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.base, self.field)
    }
}

#[derive(Debug, Clone)]
pub enum AstPath {
    Path(AstPathExpr),
    Ident(AstIdent),
}

impl AstPath {
    pub fn node_id(&self) -> NodeId {
        match self {
            AstPath::Path(path_expr) => path_expr.node_id,
            AstPath::Ident(ident) => ident.node_id,
        }
    }
}

impl Display for AstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstPath::Path(path_expr) => write!(f, "{}", path_expr),
            AstPath::Ident(ident) => write!(f, "{}", ident),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstImplicitPathExpr {
    pub node_id: NodeId,
    pub path: AstIdent,
}

#[derive(Debug, Clone)]
pub struct AstForcedTryExpr {
    pub node_id: NodeId,
    pub call_expr: Box<AstCallExpr>,
}

impl Display for AstForcedTryExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "try! {}", self.call_expr)
    }
}

#[derive(Debug, Clone)]
pub struct AstCallExpr {
    pub node_id: NodeId,
    pub callee: Box<AstExpr>,
    pub args: Vec<Box<AstExpr>>,
}

impl AstCallExpr {
    pub fn is_constraint_kw_old(&self) -> bool {
        matches!(
            &self.callee.kind,
            AstExprKind::Ident(AstIdent { text, .. }) if text == "old"
        )
    }
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
            AstExprKind::Path(field_access) => {
                write!(f, "{}::{}", field_access.base, field_access.field)
            }
            AstExprKind::ImplicitPath(implicit_field_access) => {
                write!(f, "::{}", implicit_field_access.path)
            }
            AstExprKind::ForcedTry(forced_try) => write!(f, "{}!", forced_try.call_expr),
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct AstLiteral {
    pub node_id: NodeId,
    pub value: AstLiteralKind,
}

impl Display for AstLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Debug, Clone)]
pub enum AstLiteralKind {
    Int(i64),
    Float(f64),
}

impl AstLiteralKind {
    pub fn as_int(&self) -> i64 {
        match self {
            AstLiteralKind::Int(value) => *value,
            AstLiteralKind::Float(_) => panic!("Expected int literal, found float"),
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            AstLiteralKind::Int(value) => *value as f64,
            AstLiteralKind::Float(value) => *value,
        }
    }
}

impl Display for AstLiteralKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstLiteralKind::Int(value) => write!(f, "{}", value),
            AstLiteralKind::Float(value) => write!(f, "{:?}", value),
        }
    }
}
