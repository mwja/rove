use std::{collections::HashMap, path::PathBuf};

use rust_sitter::{Spanned, errors::ParseError};

use crate::{
    SourceMap,
    ast::{self, AstPathExpr, NodeId},
    sourcemap::{SourceFileId, Span, SpanRecorder, report::Diagnostic},
};

#[rust_sitter::grammar("rove")]
mod grammar {
    use rust_sitter::Spanned;

    #[rust_sitter::language]
    pub struct Program {
        pub defs: Spanned<Vec<Def>>,
    }

    pub enum Type {
        #[rust_sitter::leaf(text = "int")]
        Int,
        #[rust_sitter::leaf(text = "float")]
        Float,
        #[rust_sitter::leaf(text = "void")]
        Void,
        #[rust_sitter::leaf(text = "bool")]
        Bool,
        Path(Box<Spanned<Path>>),
    }

    pub enum Def {
        Function(Spanned<FunctionDef>),
        Enum(Spanned<EnumDef>),
        Module(Spanned<ModuleDef>),
        Use(Spanned<UseStmt>, #[rust_sitter::leaf(text = ";")] ()),
    }

    pub struct ModuleDef {
        #[rust_sitter::leaf(text = "module")]
        _m: (),
        pub name: Spanned<Ident>,
        pub body: ModuleBlock,
    }

    pub enum ModuleBlock {
        Block {
            #[rust_sitter::leaf(text = "{")]
            _l: (),
            defs: Vec<Def>,
            #[rust_sitter::leaf(text = "}")]
            _r: (),
        },
        #[rust_sitter::prec()]
        Empty(#[rust_sitter::leaf(text = ";")] ()),
    }

    pub struct EnumDef {
        #[rust_sitter::leaf(text = "enum")]
        _e: (),
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "{")]
        _l: (),
        pub variants: Option<IdentList>,
        #[rust_sitter::leaf(text = "}")]
        _r: (),
    }

    pub struct FunctionDef {
        #[rust_sitter::leaf(text = "func")]
        _fn: (),
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "(")]
        _lp: (),
        pub args: Option<ArgDefList>,
        #[rust_sitter::leaf(text = ")")]
        _rp: (),
        pub throws: Option<ThrowsDef>,
        pub return_ty: Option<ReturnDef>,
        pub constraints: Vec<Spanned<Constraint>>,
        pub body: Spanned<BlockStmt>,
    }

    // Comma-separated lists, which permit a trailing comma. These are right
    // recursive (`item [, [list]]`) as tree-sitter can't otherwise decide
    // whether a comma is a separator or trailing with a single token lookahead.
    pub struct IdentList {
        pub head: Spanned<Ident>,
        pub tail: Option<IdentListTail>,
    }

    pub struct IdentListTail {
        #[rust_sitter::leaf(text = ",")]
        _c: (),
        pub rest: Option<Box<IdentList>>,
    }

    pub struct ArgDefList {
        pub head: Spanned<ArgDef>,
        pub tail: Option<ArgDefListTail>,
    }

    pub struct ArgDefListTail {
        #[rust_sitter::leaf(text = ",")]
        _c: (),
        pub rest: Option<Box<ArgDefList>>,
    }

    pub struct ExprList {
        pub head: Box<Spanned<Expr>>,
        pub tail: Option<ExprListTail>,
    }

    pub struct ExprListTail {
        #[rust_sitter::leaf(text = ",")]
        _c: (),
        pub rest: Option<Box<ExprList>>,
    }

    pub struct ThrowsDef {
        #[rust_sitter::leaf(text = "throws")]
        _t: (),
        pub error_type: Spanned<Type>,
    }

    pub enum Constraint {
        Require(RequireConstraint),
        Ensure(EnsureConstraint),
        // guard constraints are recoverable
        Guard(GuardConstraint),
    }

    pub struct RequireConstraint {
        #[rust_sitter::leaf(text = "require!")]
        _r: (),
        pub tag: Option<Spanned<Ident>>,
        #[rust_sitter::leaf(text = ":")]
        _c: (),
        pub expr: Spanned<Expr>,
    }

    pub struct EnsureConstraint {
        #[rust_sitter::leaf(text = "ensure!")]
        _e: (),
        pub tag: Option<Spanned<Ident>>,
        #[rust_sitter::leaf(text = ":")]
        _c: (),
        pub expr: Spanned<Expr>,
    }

    pub struct GuardConstraint {
        #[rust_sitter::leaf(text = "guard")]
        _c: (),
        pub expr: Spanned<Expr>,
        #[rust_sitter::leaf(text = "else")]
        _else: (),
        pub error: Spanned<Expr>,
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
        ImplicitReturn(Spanned<Expr>),
        Decl(Spanned<Decl>, #[rust_sitter::leaf(text = ";")] ()),
        Print(Spanned<PrintStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Assign(Spanned<AssignStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Block(Spanned<BlockStmt>),
        If(Spanned<IfStmt>),
        Loop(Spanned<LoopStmt>),
        While(Spanned<WhileStmt>),
        Break(
            #[rust_sitter::leaf(text = "break")] Spanned<()>,
            #[rust_sitter::leaf(text = ";")] (),
        ),
        Continue(
            #[rust_sitter::leaf(text = "continue")] Spanned<()>,
            #[rust_sitter::leaf(text = ";")] (),
        ),
        Fallthrough(
            #[rust_sitter::leaf(text = "fallthrough")] Spanned<()>,
            #[rust_sitter::leaf(text = ";")] (),
        ),
        Switch(Spanned<SwitchStmt>),
        Throw(Spanned<ThrowStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Guard(Spanned<GuardStmt>),
        Require(
            Spanned<RequireConstraint>,
            #[rust_sitter::leaf(text = ";")] (),
        ),
        Return(Spanned<ReturnStmt>, #[rust_sitter::leaf(text = ";")] ()),
        Use(Spanned<UseStmt>, #[rust_sitter::leaf(text = ";")] ()),
    }

    pub struct UseStmt {
        #[rust_sitter::leaf(text = "use")]
        _use: (),
        pub tree: Spanned<UseTree>,
    }

    pub enum UseTree {
        // a::<rest>
        Nested {
            name: Spanned<PathSegment>,
            #[rust_sitter::leaf(text = "::")]
            _c: (),
            rest: Box<Spanned<UseTree>>,
        },
        // {b, c::d, e as f}
        Group {
            #[rust_sitter::leaf(text = "{")]
            _l: (),
            items: Option<UseTreeList>,
            #[rust_sitter::leaf(text = "}")]
            _r: (),
        },
        // b  /  b as c
        Leaf {
            name: Spanned<PathSegment>,
            alias: Option<UseAlias>,
        },
    }

    pub struct UseAlias {
        #[rust_sitter::leaf(text = "as")]
        _as: (),
        pub name: Spanned<Ident>,
    }

    pub struct UseTreeList {
        pub head: Box<Spanned<UseTree>>,
        pub tail: Option<UseTreeListTail>,
    }

    pub struct UseTreeListTail {
        #[rust_sitter::leaf(text = ",")]
        _c: (),
        pub rest: Option<Box<UseTreeList>>,
    }

    pub struct GuardStmt {
        #[rust_sitter::leaf(text = "guard")]
        _guard: (),
        pub cond: Box<Spanned<Expr>>,
        #[rust_sitter::leaf(text = "else")]
        _else: (),
        pub else_: GuardElse,
    }

    pub enum GuardElse {
        Block(Spanned<BlockStmt>),
        // Shorthand for `else { throw X; }`, matching the header form.
        Throw(Spanned<Expr>, #[rust_sitter::leaf(text = ";")] ()),
    }

    pub struct ThrowStmt {
        #[rust_sitter::leaf(text = "throw")]
        pub _switch: (),
        pub expr: Box<Spanned<Expr>>,
    }

    pub struct SwitchStmt {
        #[rust_sitter::leaf(text = "switch")]
        pub _switch: (),
        pub expr: Box<Spanned<Expr>>,
        #[rust_sitter::leaf(text = "{")]
        pub _l: (),
        pub cases: Vec<Spanned<SwitchCase>>,
        #[rust_sitter::leaf(text = "}")]
        pub _r: (),
    }

    pub enum SwitchCase {
        Case(Spanned<Case>),
        Else(Spanned<ElseCase>),
    }

    pub struct Case {
        #[rust_sitter::leaf(text = "case")]
        _case: (),
        pub expr: Box<Spanned<Expr>>,
        #[rust_sitter::leaf(text = "=>")]
        _bar: (),
        pub body: Spanned<BlockStmt>,
    }

    pub struct ElseCase {
        #[rust_sitter::leaf(text = "else")]
        pub _else: (),
        #[rust_sitter::leaf(text = "=>")]
        _bar: (),
        pub body: Spanned<BlockStmt>,
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

    pub struct LoopStmt {
        #[rust_sitter::leaf(text = "loop")]
        pub _loop: (),
        pub body: Spanned<BlockStmt>,
    }

    pub struct WhileStmt {
        #[rust_sitter::leaf(text = "while")]
        pub _while: (),
        pub cond: Box<Spanned<Expr>>,
        pub body: Spanned<BlockStmt>,
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
        Unary(Spanned<UnaryExpr>),
        Binary(Spanned<BinaryExpr>),
        Literal(Spanned<Literal>),
        Ident(Spanned<Ident>),
        #[rust_sitter::prec_left(99)]
        Path(Spanned<PathExpr>),
        Wrapped(
            #[rust_sitter::leaf(text = "(")] (),
            Box<Spanned<Expr>>,
            #[rust_sitter::leaf(text = ")")] (),
        ),
        #[rust_sitter::prec_left(99)]
        Call(Spanned<CallExpr>),
        // TryElse(Spanned<TryElseExpr>),
        ForcedTry(Spanned<ForcedTryExpr>),
        TryCatch(Spanned<TryCatchExpr>),
    }

    #[rust_sitter::prec_left(99)]
    pub struct PathExpr {
        pub base: Option<Box<Spanned<Path>>>,
        #[rust_sitter::leaf(text = "::")]
        _colon: (),
        pub field: Spanned<PathSegment>,
    }

    // `super` is its own segment rather than an identifier, so it can only
    // ever name a module.
    pub enum PathSegment {
        Super(#[rust_sitter::leaf(text = "super")] ()),
        Ident(Spanned<Ident>),
    }

    pub enum Path {
        Path(Spanned<PathExpr>),
        #[rust_sitter::prec(2)]
        Super(#[rust_sitter::leaf(text = "super")] Spanned<()>),
        #[rust_sitter::prec(1)]
        Ident(Spanned<Ident>),
    }

    // try! dangerousfunc() => exits the same way as
    #[rust_sitter::prec_right(0)]
    pub struct ForcedTryExpr {
        #[rust_sitter::leaf(text = "try!")]
        _try: (),
        pub expr: Box<Spanned<CallExpr>>,
    }

    // let a = try dangerousfunc() catch |err| { 0 };
    #[rust_sitter::prec_right(0)]
    pub struct TryCatchExpr {
        #[rust_sitter::leaf(text = "try")]
        _try: (),
        pub expr: Box<Spanned<CallExpr>>,
        #[rust_sitter::leaf(text = "catch")]
        _catch: (),
        pub binding: Option<CatchBinding>,
        pub body: Spanned<BlockStmt>,
    }

    pub struct CatchBinding {
        #[rust_sitter::leaf(text = "|")]
        _l: (),
        pub name: Spanned<Ident>,
        #[rust_sitter::leaf(text = "|")]
        _r: (),
    }

    // // let a = try dangerousfunc() else 0
    // #[rust_sitter::prec_right(0)]
    // pub struct TryElseExpr {
    //     #[rust_sitter::leaf(text = "try")]
    //     _try: (),
    //     pub expr: Box<Spanned<CallExpr>>,
    //     #[rust_sitter::leaf(text = "else")]
    //     _else: (),
    //     pub else_: Box<Spanned<Expr>>,
    // }

    #[rust_sitter::prec_left(99)]
    pub struct CallExpr {
        pub callee: Box<Spanned<Expr>>,
        #[rust_sitter::leaf(text = "(")]
        _l: (),
        pub args: Option<ExprList>,
        #[rust_sitter::leaf(text = ")")]
        _r: (),
    }

    pub struct PrintStmt {
        #[rust_sitter::leaf(text = "print")]
        pub _p: (),
        pub expr: Box<Spanned<Expr>>,
    }

    pub enum UnaryExpr {
        #[rust_sitter::prec_left(6)]
        Not(#[rust_sitter::leaf(text = "!")] (), Box<Spanned<Expr>>),
        #[rust_sitter::prec_left(6)]
        Neg(#[rust_sitter::leaf(text = "-")] (), Box<Spanned<Expr>>),
    }

    // Tightest first, as in C/Rust: product, sum, comparison, &&, ||.
    pub enum BinaryExpr {
        #[rust_sitter::prec_left(5)]
        Product {
            left: Box<Spanned<Expr>>,
            op: Spanned<ProductOp>,
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(4)]
        Sum {
            left: Box<Spanned<Expr>>,
            op: Spanned<SumOp>,
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(3)]
        Comparison {
            left: Box<Spanned<Expr>>,
            op: Spanned<ComparisonOp>,
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(2)]
        LogicalAnd {
            left: Box<Spanned<Expr>>,
            #[rust_sitter::leaf(text = "&&")]
            op: (),
            right: Box<Spanned<Expr>>,
        },

        #[rust_sitter::prec_left(1)]
        LogicalOr {
            left: Box<Spanned<Expr>>,
            #[rust_sitter::leaf(text = "||")]
            op: (),
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
              transform = |v| v.to_string()
            )]
            String,
        ),
        Float(
            #[rust_sitter::leaf(
              pattern = r"(?:0|[1-9]\d*)\.[0-9]*|\.[0-9]+",
              transform = |v| v.parse().unwrap()
            )]
            f64,
        ),
        Bool(BoolLiteral),
    }

    pub enum BoolLiteral {
        #[rust_sitter::leaf(text = "true")]
        True,
        #[rust_sitter::leaf(text = "false")]
        False,
    }

    impl From<BoolLiteral> for bool {
        fn from(value: BoolLiteral) -> Self {
            match value {
                BoolLiteral::True => true,
                BoolLiteral::False => false,
            }
        }
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
        #[rust_sitter::leaf(pattern = r"//([^!\n][^\n]*)?")]
        _comment: (),
    }
    #[rust_sitter::extra]
    #[allow(dead_code)]
    struct MultilineComment {
        #[rust_sitter::leaf(pattern = r"/\*[^*]*\*+([^/*][^*]*\*+)*/")]
        _linecomment: (),
    }
}

/// A use-tree segment, kept owned so a prefix can be shared between siblings.
enum UseSegment {
    Super,
    Ident(String),
}

impl From<grammar::PathSegment> for UseSegment {
    fn from(segment: grammar::PathSegment) -> Self {
        match segment {
            grammar::PathSegment::Super(()) => UseSegment::Super,
            grammar::PathSegment::Ident(ident) => UseSegment::Ident(ident.value.text),
        }
    }
}

/// A right-recursive comma-separated list from the grammar.
trait CommaList: Sized {
    type Item;

    fn split(self) -> (Self::Item, Option<Self>);
}

impl CommaList for grammar::IdentList {
    type Item = Spanned<grammar::Ident>;

    fn split(self) -> (Self::Item, Option<Self>) {
        (
            self.head,
            self.tail.and_then(|tail| tail.rest).map(|rest| *rest),
        )
    }
}

impl CommaList for grammar::ArgDefList {
    type Item = Spanned<grammar::ArgDef>;

    fn split(self) -> (Self::Item, Option<Self>) {
        (
            self.head,
            self.tail.and_then(|tail| tail.rest).map(|rest| *rest),
        )
    }
}

impl CommaList for grammar::ExprList {
    type Item = Box<Spanned<grammar::Expr>>;

    fn split(self) -> (Self::Item, Option<Self>) {
        (
            self.head,
            self.tail.and_then(|tail| tail.rest).map(|rest| *rest),
        )
    }
}

impl CommaList for grammar::UseTreeList {
    type Item = Box<Spanned<grammar::UseTree>>;

    fn split(self) -> (Self::Item, Option<Self>) {
        (
            self.head,
            self.tail.and_then(|tail| tail.rest).map(|rest| *rest),
        )
    }
}

trait FlattenList {
    type Item;

    fn flatten_list(self) -> Vec<Self::Item>;
}

impl<L: CommaList> FlattenList for Option<L> {
    type Item = L::Item;

    fn flatten_list(self) -> Vec<Self::Item> {
        let mut items = Vec::new();
        let mut rest = self;
        while let Some(list) = rest {
            let (item, next) = list.split();
            items.push(item);
            rest = next;
        }
        items
    }
}

pub fn parse(
    input: &str,
    source_file_id: SourceFileId,
) -> Result<grammar::Program, Vec<Diagnostic>> {
    // focus is getting it working. pretty print later.
    let res = grammar::parse(input);
    match res {
        Ok(program) => Ok(program),
        Err(err) => {
            let mut diagnostics = Vec::new();
            fn transform_diagnostic(
                err: &ParseError,
                diag: &mut Vec<Diagnostic>,
                source_file_id: SourceFileId,
            ) {
                match err.reason {
                    rust_sitter::errors::ParseErrorReason::FailedNode(ref errs) => {
                        if errs.len() > 0 {
                            errs.iter()
                                .for_each(|err| transform_diagnostic(err, diag, source_file_id));
                        } else {
                            diag.push(Diagnostic::new("failed to parse").with_label(
                                "error occured here",
                                Span::from_span(source_file_id, (err.start, err.end)),
                            ));
                        }
                    }
                    rust_sitter::errors::ParseErrorReason::MissingToken(ref token) => {
                        diag.push(Diagnostic::new("failed to parse").with_label(
                            format!("expected {} here", token),
                            Span::from_span(source_file_id, (err.start, err.end)),
                        ));
                    }
                    rust_sitter::errors::ParseErrorReason::UnexpectedToken(ref token) => {
                        diag.push(Diagnostic::new("failed to parse").with_label(
                            format!("did not expect {} in this location", token),
                            Span::from_span(source_file_id, (err.start, err.end)),
                        ));
                    }
                };
            }

            err.iter()
                .for_each(|err| transform_diagnostic(err, &mut diagnostics, source_file_id));

            Err(diagnostics)
        }
    }
}

fn lower_to_ast(
    program: grammar::Program,
    source_file_id: SourceFileId,
    node_to_span: &mut SpanRecorder<NodeId>,
    next_node_id: &mut usize,
) -> Result<ast::AstModule, Vec<Diagnostic>> {
    let (ast, node_id) =
        ProgramLowerer::new(source_file_id, node_to_span, Some(*next_node_id)).lower(program)?;
    *next_node_id = node_id;
    Ok(ast)
}

pub fn lower_modules(
    root_path: PathBuf,
    search_path: PathBuf,
    source_map: &mut SourceMap,
    node_to_span: &mut SpanRecorder<NodeId>,
    next_node_id: &mut usize,
) -> Result<ast::AstModule, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    // root
    let input = std::fs::read(&root_path)
        .map_err(|_| vec![Diagnostic::new("unable to read input file")])?;
    let contents = String::from_utf8(input)
        .map_err(|_| vec![Diagnostic::new("unable to parse input file as utf8")])?;

    let source_file_id = source_map.add_file(root_path.clone().into_boxed_path(), contents.clone());
    let raw = parse(&contents, source_file_id)?;

    let mut ast = lower_to_ast(raw, source_file_id, node_to_span, next_node_id)?;
    // find all submodules that aren't inline, and inline them
    for module in ast.mods.iter_mut() {
        if !module.is_shell {
            continue;
        }

        let path = {
            let mut path_buf = search_path.clone();
            path_buf.push(format!("{}.rv", module.name));
            path_buf
        };
        let search_path = {
            let mut path_buf = search_path.clone();
            path_buf.push(format!("{}", module.name));
            path_buf
        };
        match lower_modules(path, search_path, source_map, node_to_span, next_node_id) {
            Err(diag) => diagnostics.extend(diag),
            Ok(submodule) => {
                let name = module.name.clone();
                *module = submodule;
                module.name = name;
            }
        }
    }

    if diagnostics.is_empty() {
        Ok(ast)
    } else {
        Err(diagnostics)
    }
}

// struct to keep track of nodes.
struct ProgramLowerer<'a> {
    next_node_id: usize,
    source_file_id: SourceFileId,
    node_to_span: &'a mut SpanRecorder<NodeId>,
    errors: Vec<Diagnostic>,
}
impl_next_id!(ProgramLowerer<'a>.next_node_id -> NodeId);

impl<'a> ProgramLowerer<'a> {
    fn new(
        source_file_id: SourceFileId,
        node_to_span: &'a mut SpanRecorder<NodeId>,
        next_node_id: Option<usize>,
    ) -> Self {
        Self {
            next_node_id: next_node_id.unwrap_or_default(),
            source_file_id,
            node_to_span,
            errors: Vec::new(),
        }
    }

    fn next_id_spanned(&mut self, span: (usize, usize)) -> NodeId {
        let id = self.next_id();
        self.node_to_span
            .record(id, Span::from_span(self.source_file_id, span));
        id
    }

    pub fn lower(
        mut self,
        program: grammar::Program,
    ) -> Result<(ast::AstModule, usize), Vec<Diagnostic>> {
        let program =
            self.lower_definitions("<root>", program.defs.span, program.defs.value, false);

        if self.errors.is_empty() {
            Ok((program, self.next_node_id))
        } else {
            Err(self.errors)
        }
    }

    fn lower_module_def(&mut self, def: grammar::Def) -> ast::AstModule {
        let grammar::Def::Module(module_def) = def else {
            unreachable!();
        };
        let is_shell = matches!(module_def.value.body, grammar::ModuleBlock::Empty(..));

        self.lower_definitions(
            module_def.value.name.value.text,
            module_def.span,
            match module_def.value.body {
                grammar::ModuleBlock::Empty(..) => vec![],
                grammar::ModuleBlock::Block { defs, .. } => defs,
            },
            is_shell,
        )
    }

    fn lower_definitions(
        &mut self,
        name: impl Into<String>,
        span: (usize, usize),
        body_defs: Vec<grammar::Def>,
        is_shell: bool,
    ) -> ast::AstModule {
        let mut defs = Vec::new();
        let mut enums = Vec::new();
        let mut modules = Vec::new();
        let mut uses = Vec::new();

        for def in body_defs {
            match def {
                grammar::Def::Function(..) => defs.push(def),
                grammar::Def::Enum(..) => enums.push(def),
                grammar::Def::Module(..) => modules.push(def),
                grammar::Def::Use(use_, _) => uses.push(use_),
            }
        }
        let program = ast::AstModule {
            node_id: self.next_id_spanned(span),
            name: name.into(),
            mods: modules
                .into_iter()
                .map(|def| self.lower_module_def(def))
                .collect(),
            defs: defs.into_iter().map(|def| self.lower_def(def)).collect(),
            enums: enums
                .into_iter()
                .map(|def| self.lower_enum_def(def))
                .collect(),
            uses: uses
                .into_iter()
                .map(|use_| self.lower_use_stmt(use_))
                .collect(),
            is_shell,
        };

        program
    }

    fn lower_enum_def(&mut self, def: grammar::Def) -> ast::AstEnumDef {
        let grammar::Def::Enum(enum_def) = def else {
            unreachable!();
        };

        ast::AstEnumDef {
            node_id: self.next_id_spanned(enum_def.span),
            name: enum_def.value.name.text.clone(),
            variants: enum_def
                .value
                .variants
                .flatten_list()
                .into_iter()
                .map(|ident| self.lower_ident(ident))
                .collect(),
        }
    }

    fn lower_def(&mut self, def: grammar::Def) -> ast::AstDef {
        let grammar::Def::Function(func) = def else {
            unreachable!();
        };

        ast::AstDef::Function(self.lower_function_def(func))
    }

    fn lower_function_def(&mut self, func: Spanned<grammar::FunctionDef>) -> ast::AstFunctionDef {
        ast::AstFunctionDef {
            node_id: self.next_id_spanned(func.span),
            name: func.value.name.text.clone(),
            return_node_id: func
                .value
                .return_ty
                .as_ref()
                .map(|r| self.next_id_spanned(r.ty.span)),
            throws_node_id: func
                .value
                .throws
                .as_ref()
                .map(|t| self.next_id_spanned(t.error_type.span)),
            throws: func
                .value
                .throws
                .map(|throws| self.lower_type(Some(throws.error_type))),
            return_ty: self.lower_type(func.value.return_ty.map(|rty| rty.ty)),
            args: func
                .value
                .args
                .flatten_list()
                .into_iter()
                .map(|arg| self.lower_arg_def(arg))
                .collect(),
            body: self.lower_block_stmt(func.value.body),
            constraints: func
                .value
                .constraints
                .into_iter()
                .map(|c| self.lower_constraint(c))
                .collect(),
        }
    }

    fn lower_constraint(&mut self, constraint: Spanned<grammar::Constraint>) -> ast::AstConstraint {
        match constraint.value {
            grammar::Constraint::Ensure(grammar::EnsureConstraint { expr, tag, .. }) => {
                ast::AstConstraint::Ensure(ast::AstEnsureConstraint {
                    condition: Box::new(self.lower_expr(expr)),
                    node_id: self.next_id_spanned(constraint.span),
                    tag: tag.map(|t| t.text.clone()),
                })
            }
            grammar::Constraint::Require(require) => {
                ast::AstConstraint::Require(self.lower_require(require, constraint.span))
            }
            grammar::Constraint::Guard(grammar::GuardConstraint { expr, error, .. }) => {
                ast::AstConstraint::Guard(ast::AstGuardConstraint {
                    condition: Box::new(self.lower_expr(expr)),
                    node_id: self.next_id_spanned(constraint.span),
                    error: self.lower_expr(error),
                })
            }
        }
    }

    fn lower_arg_def(&mut self, arg: Spanned<grammar::ArgDef>) -> ast::AstArgDef {
        ast::AstArgDef {
            node_id: self.next_id_spanned(arg.span),
            name: arg.value.name.text.clone(),
            ty: self.lower_type(Some(arg.value.ty)),
        }
    }

    fn lower_type(&mut self, ty: Option<Spanned<grammar::Type>>) -> ast::AstType {
        match ty.map(|ty| ty.value) {
            Some(grammar::Type::Float) => ast::AstType::Float,
            Some(grammar::Type::Int) => ast::AstType::Int,
            Some(grammar::Type::Void) => ast::AstType::Void,
            Some(grammar::Type::Bool) => ast::AstType::Bool,
            Some(grammar::Type::Path(path)) => ast::AstType::Path(self.lower_path(path.value)),
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
            grammar::Stmt::ImplicitReturn(expr) => {
                ast::AstStmtKind::ImplicitReturn(self.lower_expr(expr))
            }
            grammar::Stmt::Return(return_stmt, _) => {
                ast::AstStmtKind::Return(self.lower_return_stmt(return_stmt))
            }
            grammar::Stmt::Loop(loop_) => ast::AstStmtKind::Loop(self.lower_loop_stmt(loop_)),
            grammar::Stmt::While(while_) => ast::AstStmtKind::While(self.lower_while_stmt(while_)),
            grammar::Stmt::Break(v, _) => ast::AstStmtKind::Break(self.lower_break_stmt(v)),
            grammar::Stmt::Fallthrough(v, _) => {
                ast::AstStmtKind::Fallthrough(self.lower_fallthrough_stmt(v))
            }
            grammar::Stmt::Continue(v, _) => {
                ast::AstStmtKind::Continue(self.lower_continue_stmt(v))
            }
            grammar::Stmt::Switch(switch) => {
                ast::AstStmtKind::Switch(self.lower_switch_stmt(switch))
            }
            grammar::Stmt::Throw(throw, _) => ast::AstStmtKind::Throw(self.lower_throw_stmt(throw)),
            grammar::Stmt::Guard(guard) => ast::AstStmtKind::Guard(self.lower_guard_stmt(guard)),
            grammar::Stmt::Require(require, _) => {
                ast::AstStmtKind::Require(self.lower_require(require.value, require.span))
            }
            grammar::Stmt::Use(use_, _) => ast::AstStmtKind::Use(self.lower_use_stmt(use_)),
        }
    }

    fn lower_use_stmt(&mut self, use_stmt: Spanned<grammar::UseStmt>) -> ast::AstUseStmt {
        let node_id = self.next_id_spanned(use_stmt.span);
        let mut entries = Vec::new();
        self.lower_use_tree(use_stmt.value.tree, &mut Vec::new(), &mut entries);
        ast::AstUseStmt { node_id, entries }
    }

    fn lower_use_tree(
        &mut self,
        tree: Spanned<grammar::UseTree>,
        prefix: &mut Vec<(UseSegment, (usize, usize))>,
        out: &mut Vec<ast::AstUseEntry>,
    ) {
        match tree.value {
            grammar::UseTree::Nested { name, rest, .. } => {
                prefix.push((name.value.into(), name.span));
                self.lower_use_tree(*rest, prefix, out);
                prefix.pop();
            }
            grammar::UseTree::Group { items, .. } => {
                for item in items.flatten_list() {
                    self.lower_use_tree(*item, prefix, out);
                }
            }
            grammar::UseTree::Leaf { name, alias } => {
                prefix.push((name.value.into(), name.span));
                let path = self.build_path(prefix);
                prefix.pop();
                out.push(ast::AstUseEntry {
                    node_id: self.next_id_spanned(tree.span),
                    path,
                    alias: alias.map(|a| self.lower_ident(a.name)),
                });
            }
        }
    }

    // [a, b, c] -> Path(Path(Ident a, b), c), fresh ids each time
    fn build_path(&mut self, segs: &[(UseSegment, (usize, usize))]) -> ast::AstPath {
        let ((first, first_span), rest) =
            segs.split_first().unwrap_or_else(|| bug!("empty use path"));
        let mut path = match first {
            UseSegment::Super => ast::AstPath::Super(self.next_id_spanned(*first_span)),
            UseSegment::Ident(text) => ast::AstPath::Ident(ast::AstIdent {
                node_id: self.next_id_spanned(*first_span),
                text: text.clone(),
            }),
        };
        for (segment, span) in rest {
            let path_span = (first_span.0, span.1);
            path = match segment {
                UseSegment::Super => self.qualify_super(path, *span, path_span),
                UseSegment::Ident(text) => ast::AstPath::Path(ast::AstPathExpr {
                    node_id: self.next_id_spanned(path_span),
                    base: Box::new(path),
                    field: ast::AstIdent {
                        node_id: self.next_id_spanned(*span),
                        text: text.clone(),
                    },
                }),
            };
        }
        path
    }

    /// `base::super`. `super` names a parent module, so it may only start a
    /// path or follow another `super`.
    fn qualify_super(
        &mut self,
        base: ast::AstPath,
        super_span: (usize, usize),
        path_span: (usize, usize),
    ) -> ast::AstPath {
        if !base.is_super() {
            self.errors.push(
                Diagnostic::new("`super` in an invalid position").with_label(
                    "`super` can only start a path or follow another `super`",
                    Span::from_span(self.source_file_id, super_span),
                ),
            );
        }
        ast::AstPath::QualifiedSuper(ast::AstQualifiedSuper {
            node_id: self.next_id_spanned(path_span),
            base: Box::new(base),
        })
    }

    /// Reports a path ending in `super` used where a value is expected, and
    /// returns a placeholder; lowering fails, so it is never seen.
    fn super_as_value(&mut self, node_id: NodeId, span: (usize, usize)) -> ast::AstExprKind {
        self.errors.push(
            Diagnostic::new("`super` is not a value").with_label(
                "this refers to a module",
                Span::from_span(self.source_file_id, span),
            ),
        );
        ast::AstExprKind::Ident(ast::AstIdent {
            node_id,
            text: "super".to_owned(),
        })
    }

    fn lower_guard_stmt(&mut self, guard: Spanned<grammar::GuardStmt>) -> ast::AstGuardStmt {
        let node_id = self.next_id_spanned(guard.span);
        let condition = Box::new(self.lower_expr(*guard.value.cond));
        let else_ = match guard.value.else_ {
            grammar::GuardElse::Block(block) => self.lower_block_stmt(block),
            grammar::GuardElse::Throw(error, _) => ast::AstBlockStmt {
                node_id: self.next_id_spanned(error.span),
                stmts: vec![ast::AstStmt {
                    kind: ast::AstStmtKind::Throw(ast::AstThrowStmt {
                        node_id: self.next_id_spanned(error.span),
                        expr: Box::new(self.lower_expr(error)),
                    }),
                }],
            },
        };

        ast::AstGuardStmt {
            node_id,
            condition,
            else_,
        }
    }

    fn lower_require(
        &mut self,
        require: grammar::RequireConstraint,
        span: (usize, usize),
    ) -> ast::AstRequireConstraint {
        ast::AstRequireConstraint {
            condition: Box::new(self.lower_expr(require.expr)),
            node_id: self.next_id_spanned(span),
            tag: require.tag.map(|t| t.text.clone()),
        }
    }

    fn lower_throw_stmt(&mut self, throw: Spanned<grammar::ThrowStmt>) -> ast::AstThrowStmt {
        ast::AstThrowStmt {
            node_id: self.next_id_spanned(throw.span),
            expr: Box::new(self.lower_expr(*throw.value.expr)),
        }
    }
    fn lower_switch_stmt(&mut self, switch: Spanned<grammar::SwitchStmt>) -> ast::AstSwitchStmt {
        ast::AstSwitchStmt {
            node_id: self.next_id_spanned(switch.span),
            expr: Box::new(self.lower_expr(*switch.value.expr)),
            cases: switch
                .value
                .cases
                .into_iter()
                .map(|case| self.lower_switch_case(case))
                .collect(),
        }
    }

    fn lower_switch_case(&mut self, case: Spanned<grammar::SwitchCase>) -> ast::AstSwitchCase {
        match case.value {
            grammar::SwitchCase::Case(case) => ast::AstSwitchCase::Case(ast::AstSwitchCaseItem {
                node_id: self.next_id_spanned(case.span),
                expr: Box::new(self.lower_expr(*case.value.expr)),
                body: self.lower_block_stmt(case.value.body),
            }),
            grammar::SwitchCase::Else(else_case) => {
                ast::AstSwitchCase::Else(ast::AstSwitchElseCase {
                    node_id: self.next_id_spanned(else_case.span),
                    body: self.lower_block_stmt(else_case.value.body),
                })
            }
        }
    }

    fn lower_loop_stmt(&mut self, loop_stmt: Spanned<grammar::LoopStmt>) -> ast::AstLoopStmt {
        ast::AstLoopStmt {
            node_id: self.next_id_spanned(loop_stmt.span),
            body: self.lower_block_stmt(loop_stmt.value.body),
        }
    }

    fn lower_while_stmt(&mut self, while_stmt: Spanned<grammar::WhileStmt>) -> ast::AstWhileStmt {
        ast::AstWhileStmt {
            node_id: self.next_id_spanned(while_stmt.span),
            cond: Box::new(self.lower_expr(*while_stmt.value.cond)),
            body: self.lower_block_stmt(while_stmt.value.body),
        }
    }

    fn lower_fallthrough_stmt(&mut self, fallthrough_stmt: Spanned<()>) -> ast::AstFallthroughStmt {
        ast::AstFallthroughStmt {
            node_id: self.next_id_spanned(fallthrough_stmt.span),
        }
    }

    fn lower_break_stmt(&mut self, break_stmt: Spanned<()>) -> ast::AstBreakStmt {
        ast::AstBreakStmt {
            node_id: self.next_id_spanned(break_stmt.span),
        }
    }

    fn lower_continue_stmt(&mut self, continue_stmt: Spanned<()>) -> ast::AstContinueStmt {
        ast::AstContinueStmt {
            node_id: self.next_id_spanned(continue_stmt.span),
        }
    }

    fn lower_return_stmt(
        &mut self,
        return_stmt: Spanned<grammar::ReturnStmt>,
    ) -> ast::AstReturnStmt {
        ast::AstReturnStmt {
            node_id: self.next_id_spanned(return_stmt.span),
            expr: return_stmt.value.expr.map(|expr| self.lower_expr(expr)),
        }
    }

    fn lower_block_stmt(&mut self, block: Spanned<grammar::BlockStmt>) -> ast::AstBlockStmt {
        ast::AstBlockStmt {
            node_id: self.next_id_spanned(block.span),
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
            node_id: self.next_id_spanned(if_stmt.span),
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
            node_id: self.next_id_spanned(assign.span),
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
            grammar::Expr::Unary(unary) => ast::AstExprKind::Unary(self.lower_unary(unary)),
            grammar::Expr::Binary(binary) => ast::AstExprKind::Binary(self.lower_binary(binary)),
            grammar::Expr::Literal(literal) => {
                ast::AstExprKind::Literal(self.lower_literal(literal))
            }
            grammar::Expr::Ident(ident) => ast::AstExprKind::Ident(self.lower_ident(ident)),
            // we discard a node id here, but that's okay
            grammar::Expr::Wrapped(_, expr, _) => self.lower_expr(*expr).kind,
            grammar::Expr::Call(call) => ast::AstExprKind::Call(self.lower_call(call)),
            grammar::Expr::Path(field) => match field.value.base {
                Some(path_expr) => {
                    let base = self.lower_path(path_expr.value);
                    match self.qualify_path(base, field.value.field, field.span) {
                        ast::AstPath::Path(path_expr) => ast::AstExprKind::Path(path_expr),
                        path => self.super_as_value(path.node_id(), field.span),
                    }
                }
                None => match field.value.field.value {
                    grammar::PathSegment::Ident(ident) => {
                        ast::AstExprKind::ImplicitPath(ast::AstImplicitPathExpr {
                            node_id: self.next_id_spanned(field.span),
                            path: self.lower_ident(ident),
                        })
                    }
                    grammar::PathSegment::Super(()) => {
                        let node_id = self.next_id_spanned(field.span);
                        self.super_as_value(node_id, field.span)
                    }
                },
            },
            grammar::Expr::ForcedTry(forced_try) => {
                ast::AstExprKind::ForcedTry(self.lower_forced_try(forced_try))
            }
            grammar::Expr::TryCatch(try_catch) => {
                ast::AstExprKind::TryCatch(self.lower_try_catch(try_catch))
            }
        }
    }

    fn lower_path(&mut self, path: grammar::Path) -> ast::AstPath {
        match path {
            grammar::Path::Ident(ident) => ast::AstPath::Ident(self.lower_ident(ident)),
            grammar::Path::Path(path_expr) => match path_expr.value.base {
                Some(base) => {
                    let base = self.lower_path(base.value);
                    self.qualify_path(base, path_expr.value.field, path_expr.span)
                }
                None => match path_expr.value.field.value {
                    grammar::PathSegment::Ident(ident) => {
                        ast::AstPath::Ident(self.lower_ident(ident))
                    }
                    grammar::PathSegment::Super(()) => {
                        ast::AstPath::Super(self.next_id_spanned(path_expr.value.field.span))
                    }
                },
            },
            grammar::Path::Super(spanned_super) => {
                ast::AstPath::Super(self.next_id_spanned(spanned_super.span))
            }
        }
    }

    /// `base::segment`, spanning `span`.
    fn qualify_path(
        &mut self,
        base: ast::AstPath,
        segment: Spanned<grammar::PathSegment>,
        span: (usize, usize),
    ) -> ast::AstPath {
        match segment.value {
            grammar::PathSegment::Ident(ident) => ast::AstPath::Path(ast::AstPathExpr {
                node_id: self.next_id_spanned(span),
                base: Box::new(base),
                field: self.lower_ident(ident),
            }),
            grammar::PathSegment::Super(()) => self.qualify_super(base, segment.span, span),
        }
    }

    fn lower_forced_try(
        &mut self,
        forced_try: Spanned<grammar::ForcedTryExpr>,
    ) -> ast::AstForcedTryExpr {
        ast::AstForcedTryExpr {
            node_id: self.next_id_spanned(forced_try.span),
            call_expr: Box::new(self.lower_call(*forced_try.value.expr)),
        }
    }

    fn lower_try_catch(
        &mut self,
        try_catch: Spanned<grammar::TryCatchExpr>,
    ) -> ast::AstTryCatchExpr {
        ast::AstTryCatchExpr {
            node_id: self.next_id_spanned(try_catch.span),
            call_expr: Box::new(self.lower_call(*try_catch.value.expr)),
            binding: try_catch
                .value
                .binding
                .map(|binding| self.lower_ident(binding.name)),
            body: self.lower_block_stmt(try_catch.value.body),
        }
    }

    fn lower_call(&mut self, call: Spanned<grammar::CallExpr>) -> ast::AstCallExpr {
        ast::AstCallExpr {
            node_id: self.next_id_spanned(call.span),
            callee: Box::new(self.lower_expr(*call.value.callee)),
            args: call
                .value
                .args
                .flatten_list()
                .into_iter()
                .map(|arg| Box::new(self.lower_expr(*arg)))
                .collect::<Vec<_>>(),
        }
    }

    fn lower_ident(&mut self, ident: Spanned<grammar::Ident>) -> ast::AstIdent {
        ast::AstIdent {
            node_id: self.next_id_spanned(ident.span),
            text: ident.value.text,
        }
    }

    fn lower_unary(&mut self, unary: Spanned<grammar::UnaryExpr>) -> ast::AstUnaryExpr {
        match unary.value {
            grammar::UnaryExpr::Not(_, expr) => ast::AstUnaryExpr {
                node_id: self.next_id_spanned(unary.span),
                operator: ast::AstUnaryOperator::Not,
                expr: Box::new(self.lower_expr(*expr)),
            },
            grammar::UnaryExpr::Neg(_, expr) => ast::AstUnaryExpr {
                node_id: self.next_id_spanned(unary.span),
                operator: ast::AstUnaryOperator::Negate,
                expr: Box::new(self.lower_expr(*expr)),
            },
        }
    }

    fn lower_binary(&mut self, binary: Spanned<grammar::BinaryExpr>) -> ast::AstBinaryExpr {
        match binary.value {
            grammar::BinaryExpr::Product { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id_spanned(binary.span),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: match op.value {
                    grammar::ProductOp::Div => ast::AstBinaryOperator::Div,
                    grammar::ProductOp::Mul => ast::AstBinaryOperator::Mul,
                },
            },
            grammar::BinaryExpr::Sum { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id_spanned(binary.span),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: match op.value {
                    grammar::SumOp::Add => ast::AstBinaryOperator::Add,
                    grammar::SumOp::Sub => ast::AstBinaryOperator::Sub,
                },
            },
            grammar::BinaryExpr::Comparison { left, op, right } => ast::AstBinaryExpr {
                node_id: self.next_id_spanned(binary.span),
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
            grammar::BinaryExpr::LogicalAnd { left, right, .. } => ast::AstBinaryExpr {
                node_id: self.next_id_spanned(binary.span),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: ast::AstBinaryOperator::LAnd,
            },
            grammar::BinaryExpr::LogicalOr { left, right, .. } => ast::AstBinaryExpr {
                node_id: self.next_id_spanned(binary.span),
                left: Box::new(self.lower_expr(*left)),
                right: Box::new(self.lower_expr(*right)),
                operator: ast::AstBinaryOperator::LOr,
            },
        }
    }

    fn lower_literal(&mut self, literal: Spanned<grammar::Literal>) -> ast::AstLiteral {
        ast::AstLiteral {
            node_id: self.next_id_spanned(literal.span),
            value: self.lower_literal_kind(literal.value, literal.span),
        }
    }

    fn lower_literal_kind(
        &mut self,
        literal: grammar::Literal,
        span: (usize, usize),
    ) -> ast::AstLiteralKind {
        match literal {
            grammar::Literal::Int(text) => {
                ast::AstLiteralKind::Int(text.parse().unwrap_or_else(|_| {
                    // the grammar only permits digits, so this can only be an overflow
                    self.errors
                        .push(Diagnostic::new("integer literal is too large").with_label(
                            "this literal does not fit in a 64-bit integer",
                            Span::from_span(self.source_file_id, span),
                        ));
                    0
                }))
            }
            grammar::Literal::Float(value) => ast::AstLiteralKind::Float(value),
            grammar::Literal::Bool(value) => ast::AstLiteralKind::Bool(value.into()),
        }
    }

    fn lower_print_stmt(&mut self, stmt: Spanned<grammar::PrintStmt>) -> ast::AstPrintStmt {
        ast::AstPrintStmt {
            node_id: self.next_id_spanned(stmt.span),
            expr: Box::new(self.lower_expr(*stmt.value.expr)),
        }
    }
}
