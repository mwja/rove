use std::fmt::Display;

indexable_id!(pub NodeId);

#[derive(Debug)]
pub struct AstModule {
    pub name: String,
    pub defs: Vec<AstDef>,
    pub enums: Vec<AstEnumDef>,
    pub mods: Vec<AstModule>,
    pub uses: Vec<AstUseStmt>,
    pub node_id: NodeId,
    /// to be imported?
    pub is_shell: bool,
}

#[derive(Debug)]
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
    Bool,
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
                AstType::Bool => "bool".to_owned(),
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
    pub fn name(&self) -> &str {
        match self {
            AstDef::Function(func) => &func.name,
        }
    }
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
                write!(f, "guard {} else {}", c.condition, c.error)
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

impl Display for AstModule {
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

#[derive(Debug, Clone)]
pub struct AstStmt {
    pub kind: AstStmtKind,
}

impl AstStmt {
    pub fn inner_node_id(&self) -> NodeId {
        match &self.kind {
            AstStmtKind::Expr(expr) => expr.node_id(),
            AstStmtKind::BlockLike(expr) => expr.node_id(),
            AstStmtKind::Print(print) => print.node_id,
            AstStmtKind::Decl(decl) => decl.inner_node_id(),
            AstStmtKind::Assign(assign) => assign.node_id,
            AstStmtKind::ImplicitReturn(expr) => expr.node_id(),
            AstStmtKind::Guard(guard_stmt) => guard_stmt.node_id,
            AstStmtKind::Require(require) => require.node_id,
            AstStmtKind::Use(use_stmt) => use_stmt.node_id,
        }
    }
}

impl Display for AstStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(Debug, Clone)]
pub enum AstStmtKind {
    /// `expr;`, which discards the value.
    Expr(AstExpr),
    /// A block-like expression (`if`, `switch`, `loop`, ...) in statement
    /// position without a `;`. It has nowhere to send a value, so may not have
    /// one.
    BlockLike(AstExpr),
    Print(AstPrintStmt),
    Decl(AstDecl),
    Assign(AstAssignStmt),
    ImplicitReturn(AstExpr),
    Guard(AstGuardStmt),
    Require(AstRequireConstraint),
    Use(AstUseStmt),
}

impl Display for AstStmtKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstStmtKind::Expr(expr) => write!(f, "{};", expr),
            AstStmtKind::BlockLike(expr) => write!(f, "{}", expr),
            AstStmtKind::Print(print) => write!(f, "{};", print),
            AstStmtKind::Decl(decl) => write!(f, "{};", decl),
            AstStmtKind::Assign(assign) => write!(f, "{};", assign),
            AstStmtKind::ImplicitReturn(expr) => write!(f, "{}", expr),
            AstStmtKind::Guard(guard_stmt) => write!(f, "{}", guard_stmt),
            AstStmtKind::Require(require) => write!(
                f,
                "require! {}: {};",
                require.tag.as_deref().unwrap_or(""),
                require.condition
            ),
            AstStmtKind::Use(use_stmt) => write!(f, "{};", use_stmt),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstUseStmt {
    pub node_id: NodeId,
    pub entries: Vec<AstUseEntry>,
}

#[derive(Debug, Clone)]
pub struct AstUseEntry {
    pub node_id: NodeId,
    pub path: AstPath,
    pub alias: Option<AstIdent>,
}

impl AstUseEntry {
    pub fn name(&self) -> &str {
        self.alias
            .as_ref()
            .map(|alias| alias.text.as_str())
            .unwrap_or_else(|| self.path.name())
    }
}

impl Display for AstUseStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "use {};",
            self.entries
                .iter()
                .map(|entry| {
                    if let Some(alias) = &entry.alias {
                        format!("{} as {}", entry.path, alias)
                    } else {
                        format!("{}", entry.path)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// `guard cond else ::E;` is lowered as `guard cond else { throw ::E; }`.
#[derive(Debug, Clone)]
pub struct AstGuardStmt {
    pub node_id: NodeId,
    pub condition: Box<AstExpr>,
    pub else_: AstBlockStmt,
}

impl Display for AstGuardStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "guard {} else {}", self.condition, self.else_)
    }
}

#[derive(Debug, Clone)]
pub struct AstThrowStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

impl Display for AstThrowStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "throw {}", self.expr)
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct AstSwitchElseCase {
    pub node_id: NodeId,
    pub body: AstBlockStmt,
}

impl Display for AstSwitchElseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "else: {}", self.body)
    }
}

#[derive(Debug, Clone)]
pub struct AstLoopStmt {
    pub node_id: NodeId,
    pub body: AstBlockStmt,
}

impl Display for AstLoopStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loop {{ {} }}", self.body)
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct AstBreakStmt {
    pub node_id: NodeId,
}

impl Display for AstBreakStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "break")
    }
}

#[derive(Debug, Clone)]
pub struct AstFallthroughStmt {
    pub node_id: NodeId,
}

impl Display for AstFallthroughStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fallthrough")
    }
}

#[derive(Debug, Clone)]
pub struct AstContinueStmt {
    pub node_id: NodeId,
}

impl Display for AstContinueStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "continue")
    }
}

#[derive(Debug, Clone)]
pub struct AstReturnStmt {
    /// Only for diagnostics, do NOT type this
    pub node_id: NodeId,
    pub expr: Option<Box<AstExpr>>,
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

#[derive(Debug, Clone)]
pub struct AstBlockStmt {
    pub node_id: NodeId,
    pub stmts: Vec<AstStmt>,
}

impl AstBlockStmt {
    pub fn tail(&self) -> Option<&AstExpr> {
        match self.stmts.last().map(|stmt| &stmt.kind) {
            Some(AstStmtKind::ImplicitReturn(tail)) => Some(tail),
            _ => None,
        }
    }
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
            AstExprKind::Unary(unary_expr) => unary_expr.node_id,
            AstExprKind::Literal(lit) => lit.node_id,
            AstExprKind::Binary(bin_expr) => bin_expr.node_id,
            AstExprKind::Ident(ident) => ident.node_id,
            AstExprKind::Call(call) => call.node_id,
            AstExprKind::Path(field_access) => field_access.node_id,
            AstExprKind::ImplicitPath(implicit_field_access) => implicit_field_access.node_id, // AstExprKind::ForcedTry(forced_try) => forced_try.node_id,
            AstExprKind::ForcedTry(forced_try) => forced_try.node_id,
            AstExprKind::TryCatch(try_catch) => try_catch.node_id,
            AstExprKind::Block(block) => block.node_id,
            AstExprKind::If(if_expr) => if_expr.node_id,
            AstExprKind::Loop(loop_expr) => loop_expr.node_id,
            AstExprKind::While(while_expr) => while_expr.node_id,
            AstExprKind::Switch(switch_expr) => switch_expr.node_id,
            AstExprKind::Return(return_expr) => return_expr.node_id,
            AstExprKind::Break(break_expr) => break_expr.node_id,
            AstExprKind::Continue(continue_expr) => continue_expr.node_id,
            AstExprKind::Fallthrough(fallthrough_expr) => fallthrough_expr.node_id,
            AstExprKind::Throw(throw_expr) => throw_expr.node_id,
        }
    }

    /// Block-like expressions end in a `}`, so need no `;` in statement
    /// position.
    pub fn is_block_like(&self) -> bool {
        matches!(
            self.kind,
            AstExprKind::Block(_)
                | AstExprKind::If(_)
                | AstExprKind::Loop(_)
                | AstExprKind::While(_)
                | AstExprKind::Switch(_)
        )
    }
}

impl Display for AstExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(Debug, Clone)]
pub struct AstPrintStmt {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
}

impl Display for AstPrintStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "print {}", self.expr)
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
    Unary(AstUnaryExpr),
    Binary(AstBinaryExpr),
    Literal(AstLiteral),
    Ident(AstIdent),
    Call(AstCallExpr),
    Path(AstPathExpr),
    ImplicitPath(AstImplicitPathExpr),
    ForcedTry(AstForcedTryExpr),
    TryCatch(AstTryCatchExpr),
    Block(AstBlockStmt),
    If(AstIfStmt),
    Loop(AstLoopStmt),
    While(AstWhileStmt),
    Switch(AstSwitchStmt),
    Return(AstReturnStmt),
    Break(AstBreakStmt),
    Continue(AstContinueStmt),
    Fallthrough(AstFallthroughStmt),
    Throw(AstThrowStmt),
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
    Super(NodeId),
    QualifiedSuper(AstQualifiedSuper),
    Ident(AstIdent),
}

/// `base::super`, where `base` is itself `super` or another `QualifiedSuper`.
#[derive(Debug, Clone)]
pub struct AstQualifiedSuper {
    pub node_id: NodeId,
    pub base: Box<AstPath>,
}

impl AstPath {
    pub fn node_id(&self) -> NodeId {
        match self {
            AstPath::Path(path_expr) => path_expr.node_id,
            AstPath::Super(node_id) => *node_id,
            AstPath::QualifiedSuper(qualified) => qualified.node_id,
            AstPath::Ident(ident) => ident.node_id,
        }
    }

    /// Whether this path names a module through `super`.
    pub fn is_super(&self) -> bool {
        matches!(self, AstPath::Super(_) | AstPath::QualifiedSuper(_))
    }

    pub fn name(&self) -> &str {
        match self {
            AstPath::Path(path_expr) => &path_expr.field.text,
            AstPath::Super(_) | AstPath::QualifiedSuper(_) => "super",
            AstPath::Ident(ident) => &ident.text,
        }
    }
}

impl Display for AstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstPath::Path(path_expr) => write!(f, "{}", path_expr),
            AstPath::Super(_) => write!(f, "super"),
            AstPath::QualifiedSuper(qualified) => write!(f, "{}::super", qualified.base),
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
pub struct AstTryCatchExpr {
    pub node_id: NodeId,
    pub call_expr: Box<AstCallExpr>,
    pub binding: Option<AstIdent>,
    pub body: AstBlockStmt,
}

impl Display for AstTryCatchExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "try {} catch ", self.call_expr)?;
        if let Some(binding) = &self.binding {
            write!(f, "|{}| ", binding)?;
        }
        write!(f, "{}", self.body)
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
            AstExprKind::Unary(unary) => write!(f, "{}", unary),
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
            AstExprKind::TryCatch(try_catch) => write!(f, "{}", try_catch),
            AstExprKind::Block(block) => write!(f, "{}", block),
            AstExprKind::If(if_expr) => write!(f, "{}", if_expr),
            AstExprKind::Loop(loop_expr) => write!(f, "{}", loop_expr),
            AstExprKind::While(while_expr) => write!(f, "{}", while_expr),
            AstExprKind::Switch(switch_expr) => write!(f, "{}", switch_expr),
            AstExprKind::Return(return_expr) => write!(f, "{}", return_expr),
            AstExprKind::Break(break_expr) => write!(f, "{}", break_expr),
            AstExprKind::Continue(continue_expr) => write!(f, "{}", continue_expr),
            AstExprKind::Fallthrough(fallthrough_expr) => write!(f, "{}", fallthrough_expr),
            AstExprKind::Throw(throw_expr) => write!(f, "{}", throw_expr),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstUnaryExpr {
    pub node_id: NodeId,
    pub expr: Box<AstExpr>,
    pub operator: AstUnaryOperator,
}

impl Display for AstUnaryExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}{})", self.operator, self.expr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AstUnaryOperator {
    Negate,
    Not,
}

impl Display for AstUnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op = match self {
            AstUnaryOperator::Negate => "-",
            AstUnaryOperator::Not => "!",
        };
        write!(f, "{}", op)
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
    LAnd,
    LOr,
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
            AstBinaryOperator::LAnd => "&&",
            AstBinaryOperator::LOr => "||",
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
    Bool(bool),
}

impl AstLiteralKind {
    pub fn as_int(&self) -> i64 {
        match self {
            AstLiteralKind::Int(value) => *value,
            AstLiteralKind::Bool(_) => bug!("Expected int literal, found bool"),
            AstLiteralKind::Float(_) => bug!("Expected int literal, found float"),
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            AstLiteralKind::Int(value) => *value as f64,
            AstLiteralKind::Bool(_) => bug!("Expected float literal, found bool"),
            AstLiteralKind::Float(value) => *value,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            AstLiteralKind::Int(_) => bug!("Expected bool literal, found int"),
            AstLiteralKind::Bool(value) => *value,
            AstLiteralKind::Float(_) => bug!("Expected bool literal, found float"),
        }
    }
}

impl Display for AstLiteralKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstLiteralKind::Int(value) => write!(f, "{}", value),
            AstLiteralKind::Float(value) => write!(f, "{:?}", value),
            AstLiteralKind::Bool(value) => write!(f, "{}", value),
        }
    }
}
