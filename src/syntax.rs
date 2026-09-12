use rust_sitter::Spanned;

use crate::ast::{self, NodeId};

#[rust_sitter::grammar("rove")]
mod grammar {
    use rust_sitter::Spanned;

    #[rust_sitter::language]
    pub struct Program {
        pub defs: Vec<Def>,
    }

    pub enum Type {
        #[rust_sitter::leaf(text = "int")]
        Int,
        #[rust_sitter::leaf(text = "float")]
        Float,
    }

    pub enum Def {
        Function(Spanned<FunctionDef>),
    }

    pub struct FunctionDef {
        #[rust_sitter::leaf(text = "func")]
        _fn: (),
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "(")]
        _lp: (),
        #[rust_sitter::delimited(
            #[rust_sitter::leaf(text = ",")]
            ()
        )]
        pub args: Vec<Spanned<ArgDef>>,
        #[rust_sitter::leaf(text = ")")]
        _rp: (),
        pub return_ty: Option<ReturnDef>,
        pub body: Spanned<BlockStmt>,
    }

    pub struct ArgDef {
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = ":")]
        _cl: (),
        pub ty: Spanned<Type>,
    }

    pub struct ReturnDef {
        #[rust_sitter::leaf(text = "->")]
        _arrow: (),
        pub ty: Spanned<Type>,
    }

    pub enum Stmt {
        Expr(Spanned<Expr>, #[rust_sitter::leaf(text = ";")] ()),
        Decl(Spanned<Decl>, #[rust_sitter::leaf(text = ";")] ()),
        Print(Spanned<PrintStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Assign(Spanned<AssignStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Block(Spanned<BlockStmt>),
        If(Spanned<IfStmt>),
        Return(Spanned<ReturnStmt>, #[rust_sitter::leaf(text = ";")] ()),
    }

    pub struct ReturnStmt {
        #[rust_sitter::leaf(text = "return")]
        pub _return: (),
        pub expr: Option<Spanned<Expr>>,
    }

    pub struct IfStmt {
        #[rust_sitter::leaf(text = "if")]
        pub _if: (),
        pub cond: Box<Spanned<Expr>>,
        pub then: Spanned<BlockStmt>,
        pub else_: Option<ElsePath>,
    }

    pub struct ElsePath {
        #[rust_sitter::leaf(text = "else")]
        pub _else: (),
        pub branch: ElseBranch,
    }

    // `else` is followed by either a block, or directly another `if`
    // (so `else if ... { }` chains still work without extra braces).
    pub enum ElseBranch {
        Block(Spanned<BlockStmt>),
        ElseIf(Box<Spanned<IfStmt>>),
    }

    pub struct BlockStmt {
        #[rust_sitter::leaf(text = "{")]
        pub _b: (),
        pub stmts: Vec<Spanned<Stmt>>,
        #[rust_sitter::leaf(text = "}")]
        pub _e: (),
    }

    pub struct AssignStmt {
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "=")]
        _e: (),
        pub expr: Box<Spanned<Expr>>,
    }

    pub enum Decl {
        Let(Spanned<LetDecl>),
    }

    pub struct LetDecl {
        #[rust_sitter::leaf(text = "let")]
        _l: (),
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "=")]
        _e: (),
        pub expr: Box<Spanned<Expr>>,
    }

    pub struct Ident {
        #[rust_sitter::leaf(pattern = r"[a-zA-Z_][a-zA-Z0-9_]*", transform = |text: &str| text.to_string())]
        pub text: String,
    }

    pub enum Expr {
        Binary(Spanned<BinaryExpr>),
        Literal(Spanned<Literal>),
        Ident(Spanned<Ident>),
        Wrapped(
            #[rust_sitter::leaf(text = "(")] (),
            Box<Spanned<Expr>>,
            #[rust_sitter::leaf(text = ")")] (),
        ),
        Call(Spanned<CallExpr>),
    }

    pub struct CallExpr {
        pub callee: Box<Spanned<Expr>>,
        #[rust_sitter::leaf(text = "(")]
        _l: (),
        #[rust_sitter::delimited(
            #[rust_sitter::leaf(text = ",")]
            ()
        )]
        pub args: Vec<Box<Spanned<Expr>>>,
        #[rust_sitter::leaf(text = ")")]
        _r: (),
    }

    pub struct PrintStmt {
        #[rust_sitter::leaf(text = "print")]
        pub _p: (),
        pub expr: Box<Spanned<Expr>>,
    }

    pub enum BinaryExpr {
        #[rust_sitter::prec_left(2)]
        Product {
            left: Box<Spanned<Expr>>,
            op: Spanned<ProductOp>,
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(1)]
        Sum {
            left: Box<Spanned<Expr>>,
            op: Spanned<SumOp>,
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(0)]
        Comparison {
            left: Box<Spanned<Expr>>,
            op: Spanned<ComparisonOp>,
            right: Box<Spanned<Expr>>,
        },
    }

    pub enum ComparisonOp {
        #[rust_sitter::leaf(text = "==")]
        Eq,
        #[rust_sitter::leaf(text = "!=")]
        Ne,
        #[rust_sitter::leaf(text = "<")]
        Lt,
        #[rust_sitter::leaf(text = ">")]
        Gt,
        #[rust_sitter::leaf(text = "<=")]
        Le,
        #[rust_sitter::leaf(text = ">=")]
        Ge,
    }

    pub enum Literal {
        Int(
            #[rust_sitter::leaf(
              pattern = r"0|[1-9]\d*",
              transform = |v| v.parse().unwrap()
            )]
            i64,
        ),
        Float(
            #[rust_sitter::leaf(
              pattern = r"(?:0|[1-9]\d*)?\.[0-9]*",
              transform = |v| v.parse().unwrap()
            )]
            f64,
        ),
    }

    pub enum ProductOp {
        #[rust_sitter::leaf(text = "*")]
        Mul,
        #[rust_sitter::leaf(text = "/")]
        Div,
    }

    pub enum SumOp {
        #[rust_sitter::leaf(text = "+")]
        Add,
        #[rust_sitter::leaf(text = "-")]
        Sub,
    }

    #[rust_sitter::extra]
    #[allow(dead_code)]
    struct Whitespace {
        #[rust_sitter::leaf(pattern = r"\s")]
        _whitespace: (),
    }

    #[rust_sitter::extra]
    #[allow(dead_code)]
    struct LineComment {
        #[rust_sitter::leaf(pattern = r"//([^!\n][^\n]*|)\n")]
        _comment: (),
    }
    #[rust_sitter::extra]
    #[allow(dead_code)]
    struct MultilineComment {
        #[rust_sitter::leaf(pattern = r"/\*[^\*/]\*/")]
        _linecomment: (),
    }
}

pub fn parse(input: &str) -> grammar::Program {
    // focus is getting it working. pretty print later.
    grammar::parse(input).expect("to parse correctly")
}

pub fn lower_to_ast(program: grammar::Program) -> ast::AstProgram {
    ProgramLowerer::new().lower(program)
}

// struct to keep track of nodes.
struct ProgramLowerer {
    next_node_id: usize,
}
impl_next_id!(ProgramLowerer.next_node_id -> NodeId);

impl ProgramLowerer {
    fn new() -> Self {
        Self { next_node_id: 0 }
    }

    pub fn lower(mut self, program: grammar::Program) -> ast::AstProgram {
        ast::AstProgram {
            defs: program
                .defs
                .into_iter()
                .map(|def| self.lower_def(def))
                .collect(),
        }
    }

    fn lower_def(&mut self, def: grammar::Def) -> ast::AstDef {
        match def {
            grammar::Def::Function(func) => ast::AstDef::Function(self.lower_function_def(func)),
        }
    }

    fn lower_function_def(&mut self, func: Spanned<grammar::FunctionDef>) -> ast::AstFunctionDef {
        ast::AstFunctionDef {
            node_id: self.next_id(),
            name: func.value.name.text.clone(),
            args: func
                .value
                .args
                .into_iter()
                .map(|arg| self.lower_arg_def(arg))
                .collect(),
            return_ty: self.lower_type(func.value.return_ty.and_then(|rty| Some(rty.ty))),
            body: self.lower_block_stmt(func.value.body),
            is_main: func.value.name.text == "main",
        }
    }

    fn lower_arg_def(&mut self, arg: Spanned<grammar::ArgDef>) -> ast::AstArgDef {
        ast::AstArgDef {
            node_id: self.next_id(),
            name: arg.value.name.text.clone(),
            ty: self.lower_type(Some(arg.value.ty)),
        }
    }

    fn lower_type(&mut self, ty: Option<Spanned<grammar::Type>>) -> ast::AstType {
        match ty.and_then(|ty| Some(ty.value)) {
            Some(grammar::Type::Float) => ast::AstType::Float,
            Some(grammar::Type::Int) => ast::AstType::Int,
            None => ast::AstType::Void,
        }
    }

    fn lower_stmt(&mut self, stmt: Spanned<grammar::Stmt>) -> ast::AstStmt {
        ast::AstStmt {
            kind: self.lower_stmt_kind(stmt.value),
        }
    }

    fn lower_stmt_kind(&mut self, stmt: grammar::Stmt) -> ast::AstStmtKind {
        match stmt {
            grammar::Stmt::Expr(expr, _) => ast::AstStmtKind::Expr(self.lower_expr(expr)),
            grammar::Stmt::Print(print, _) => ast::AstStmtKind::Print(self.lower_print_stmt(print)),
            grammar::Stmt::Decl(decl, _) => ast::AstStmtKind::Decl(self.lower_decl(decl)),
            grammar::Stmt::Assign(assign, _) => {
                ast::AstStmtKind::Assign(self.lower_assign_stmt(assign))
            }
            grammar::Stmt::Block(block) => ast::AstStmtKind::Block(self.lower_block_stmt(block)),
            grammar::Stmt::If(if_stmt) => ast::AstStmtKind::If(self.lower_if_stmt(if_stmt)),
            grammar::Stmt::Return(return_stmt, _) => {
                ast::AstStmtKind::Return(self.lower_return_stmt(return_stmt))
            }
        }
    }

    fn lower_return_stmt(
        &mut self,
        return_stmt: Spanned<grammar::ReturnStmt>,
    ) -> ast::AstReturnStmt {
        ast::AstReturnStmt {
            expr: return_stmt.value.expr.map(|expr| self.lower_expr(expr)),
        }
    }

    fn lower_block_stmt(&mut self, block: Spanned<grammar::BlockStmt>) -> ast::AstBlockStmt {
        ast::AstBlockStmt {
            node_id: self.next_id(),
            stmts: block
                .value
                .stmts
                .into_iter()
                .map(|stmt| self.lower_stmt(stmt))
                .collect(),
        }
    }

    fn lower_if_stmt(&mut self, if_stmt: Spanned<grammar::IfStmt>) -> ast::AstIfStmt {
        ast::AstIfStmt {
            node_id: self.next_id(),
            cond: Box::new(self.lower_expr(*if_stmt.value.cond)),
            then: Box::new(self.lower_block_stmt(if_stmt.value.then)),
            else_: if_stmt
                .value
                .else_
                .map(|e| self.lower_else_branch(e.branch)),
        }
    }

    fn lower_else_branch(&mut self, branch: grammar::ElseBranch) -> ast::AstElseBranch {
        match branch {
            grammar::ElseBranch::Block(block) => {
                ast::AstElseBranch::Block(self.lower_block_stmt(block))
            }
            grammar::ElseBranch::ElseIf(if_stmt) => {
                ast::AstElseBranch::If(Box::new(self.lower_if_stmt(*if_stmt)))
            }
        }
    }

    fn lower_assign_stmt(&mut self, assign: Spanned<grammar::AssignStmt>) -> ast::AstAssignStmt {
        ast::AstAssignStmt {
            node_id: self.next_id(),
            name: self.lower_ident(assign.value.name),
            expr: Box::new(self.lower_expr(*assign.value.expr)),
        }
    }

    fn lower_decl(&mut self, decl: Spanned<grammar::Decl>) -> ast::AstDecl {
        ast::AstDecl {
            kind: self.lower_decl_kind(decl.value),
        }
    }

    fn lower_decl_kind(&mut self, decl: grammar::Decl) -> ast::AstDeclKind {
        match decl {
            grammar::Decl::Let(let_decl) => ast::AstDeclKind::Let(self.lower_let_decl(let_decl)),
        }
    }

    fn lower_let_decl(&mut self, let_decl: Spanned<grammar::LetDecl>) -> ast::AstLetDecl {
        ast::AstLetDecl {
            name: self.lower_ident(let_decl.value.name),
            expr: Box::new(self.lower_expr(*let_decl.value.expr)),
        }
    }

    fn lower_expr(&mut self, expr: Spanned<grammar::Expr>) -> ast::AstExpr {
        ast::AstExpr {
            kind: self.lower_expr_kind(expr.value),
        }
    }

    fn lower_expr_kind(&mut self, expr: grammar::Expr) -> ast::AstExprKind {
        match expr {
            grammar::Expr::Binary(binary) => ast::AstExprKind::Binary(self.lower_binary(binary)),
            grammar::Expr::Literal(literal) => {
                ast::AstExprKind::Literal(self.lower_literal(literal))
            }
            grammar::Expr::Ident(ident) => ast::AstExprKind::Ident(self.lower_ident(ident)),
            // we discard a node id here, but that's okay
            grammar::Expr::Wrapped(_, expr, _) => self.lower_expr(*expr).kind,
            grammar::Expr::Call(call) => ast::AstExprKind::Call(self.lower_call(call.value)),
        }
    }

    fn lower_call(&mut self, call: grammar::CallExpr) -> ast::AstCallExpr {
        ast::AstCallExpr {
            node_id: self.next_id(),
            callee: Box::new(self.lower_expr(*call.callee)),
            args: call
                .args
                .into_iter()
                .map(|arg| Box::new(self.lower_expr(*arg)))
                .collect::<Vec<_>>(),
        }
    }

    fn lower_ident(&mut self, ident: Spanned<grammar::Ident>) -> ast::AstIdent {
        ast::AstIdent {
            node_id: self.next_id(),
            text: ident.value.text,
        }
    }

    fn lower_binary(&mut self, binary: Spanned<grammar::BinaryExpr>) -> ast::AstBinaryExpr {
        match binary.value {
            grammar::BinaryExpr::Product { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id(),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: match op.value {
                    grammar::ProductOp::Div => ast::AstBinaryOperator::Div,
                    grammar::ProductOp::Mul => ast::AstBinaryOperator::Mul,
                },
            },
            grammar::BinaryExpr::Sum { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id(),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: match op.value {
                    grammar::SumOp::Add => ast::AstBinaryOperator::Add,
                    grammar::SumOp::Sub => ast::AstBinaryOperator::Sub,
                },
            },
            grammar::BinaryExpr::Comparison { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id(),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: match op.value {
                    grammar::ComparisonOp::Eq => ast::AstBinaryOperator::Eq,
                    grammar::ComparisonOp::Ne => ast::AstBinaryOperator::Ne,
                    grammar::ComparisonOp::Lt => ast::AstBinaryOperator::Lt,
                    grammar::ComparisonOp::Gt => ast::AstBinaryOperator::Gt,
                    grammar::ComparisonOp::Le => ast::AstBinaryOperator::Le,
                    grammar::ComparisonOp::Ge => ast::AstBinaryOperator::Ge,
                },
            },
        }
    }

    fn lower_literal(&mut self, literal: Spanned<grammar::Literal>) -> ast::AstLiteral {
        ast::AstLiteral {
            node_id: self.next_id(),
            value: self.lower_literal_kind(literal.value),
        }
    }

    fn lower_literal_kind(&mut self, literal: grammar::Literal) -> ast::AstLiteralKind {
        match literal {
            grammar::Literal::Int(value) => ast::AstLiteralKind::Int(value),
            grammar::Literal::Float(value) => ast::AstLiteralKind::Float(value),
        }
    }

    fn lower_print_stmt(&mut self, stmt: Spanned<grammar::PrintStmt>) -> ast::AstPrintStmt {
        ast::AstPrintStmt {
            node_id: self.next_id(),
            expr: Box::new(self.lower_expr(*stmt.value.expr)),
        }
    }
}
