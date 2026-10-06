; Node names come from the rust-sitter definitions in src/syntax.rs: every
; keyword and punctuation token is its own named node (e.g. `FunctionDef__fn`).
; Generic captures come first so the more specific ones below override them.

(Ident_text) @variable

; Comments

(LineComment) @comment
(MultilineComment) @comment

; Literals

(Literal_Int_0) @number
(Literal_Float_0) @number
(BoolLiteral) @boolean

; Types

[
  (Type_Int_unit)
  (Type_Float_unit)
  (Type_Void_unit)
  (Type_Bool_unit)
] @type.builtin

(Type_Path (Path (Path_Ident (Ident (Ident_text) @type))))
(Type_Path (Path (Path_Path (PathExpr field: (PathSegment (PathSegment_Ident (Ident (Ident_text) @type)))))))

; Definitions

(FunctionDef name: (Ident (Ident_text) @function.definition))
(EnumDef name: (Ident (Ident_text) @type))
(EnumDef variants: (IdentList head: (Ident (Ident_text) @variant)))
(IdentListTail rest: (IdentList head: (Ident (Ident_text) @variant)))
(ModuleDef name: (Ident (Ident_text) @module))
(ArgDef name: (Ident (Ident_text) @variable.parameter))
(RequireConstraint tag: (Ident (Ident_text) @label))
(EnsureConstraint tag: (Ident (Ident_text) @label))

; Paths: `a::b::C`, `::Variant`

(PathExpr base: (Path (Path_Ident (Ident (Ident_text) @module))))
(PathExpr field: (PathSegment (PathSegment_Ident (Ident (Ident_text) @variant))))
(UseTree_Nested name: (PathSegment (PathSegment_Ident (Ident (Ident_text) @module))))

; Calls

(CallExpr callee: (Expr (Expr_Ident (Ident (Ident_text) @function))))
(CallExpr callee: (Expr (Expr_Path (PathExpr field: (PathSegment (PathSegment_Ident (Ident (Ident_text) @function)))))))

; Fields

(Place_FieldAccess field: (Ident (Ident_text) @property))

; `ret` and `old(x)` inside contracts

((Ident_text) @variable.special
  (#eq? @variable.special "ret"))
(CallExpr callee: (Expr (Expr_Ident (Ident (Ident_text) @function.builtin)))
  (#eq? @function.builtin "old"))

; Keywords

[
  (ModuleDef__m)
  (UseStmt__use)
  (UseAlias__as)
] @keyword.import

[
  (FunctionDef__fn)
  (EnumDef__e)
  (LetDecl__l)
  (VarDecl__v)
  (ThrowsDef__t)
  (PrintStmt__p)
] @keyword

[
  (IfStmt__if)
  (ElsePath__else)
  (SwitchStmt__switch)
  (Case__case)
  (ElseCase__else)
  (Expr_Fallthrough_0)
] @keyword.conditional

[
  (LoopStmt__loop)
  (WhileStmt__while)
  (Expr_Break_0)
  (Expr_Continue_0)
] @keyword.repeat

[
  (ReturnStmt__return)
  (ThrowStmt__switch)
] @keyword.return

[
  (TryCatchExpr__try)
  (TryCatchExpr__catch)
] @keyword.exception

; Anything that can crash is marked with `!`
[
  (ForcedTryExpr__try)
  (RequireConstraint__r)
  (EnsureConstraint__e)
] @keyword.exception

[
  (GuardConstraint__c)
  (GuardConstraint__else)
  (GuardStmt__guard)
  (GuardStmt__else)
] @keyword.conditional

[
  (PathSegment_Super_0)
  (Path_Super_0)
] @variable.special

; Operators

[
  (SumOp)
  (ProductOp)
  (ComparisonOp)
  (BinaryExpr_LogicalAnd_op)
  (BinaryExpr_LogicalOr_op)
  (UnaryExpr_Not_0)
  (UnaryExpr_Neg_0)
  (AssignStmt__e)
  (LetDecl__e)
  (VarDecl__e)
  (ReturnDef__arrow)
  (Case__bar)
  (ElseCase__bar)
] @operator

; Punctuation

[
  (FunctionDef__lp)
  (FunctionDef__rp)
  (CallExpr__l)
  (CallExpr__r)
  (Expr_Wrapped_0)
  (Expr_Wrapped_2)
] @punctuation.bracket

[
  (BlockStmt__b)
  (BlockStmt__e)
  (ModuleBlock_Block__l)
  (ModuleBlock_Block__r)
  (EnumDef__l)
  (EnumDef__r)
  (SwitchStmt__l)
  (SwitchStmt__r)
  (UseTree_Group__l)
  (UseTree_Group__r)
  (CatchBinding__l)
  (CatchBinding__r)
] @punctuation.bracket

[
  (IdentListTail__c)
  (ArgDefListTail__c)
  (ExprListTail__c)
  (UseTreeListTail__c)
] @punctuation.delimiter

[
  (Def_Use_1)
  (ModuleBlock_Empty_0)
  (Stmt_Expr_1)
  (Stmt_Decl_1)
  (Stmt_Print_1)
  (Stmt_Assign_1)
  (Stmt_Require_1)
  (Stmt_Use_1)
  (GuardElse_Throw_1)
] @punctuation.delimiter

[
  (ArgDef__cl)
  (RequireConstraint__c)
  (EnsureConstraint__c)
  (PathExpr__colon)
  (UseTree_Nested__c)
  (Place_FieldAccess__dot)
] @punctuation.delimiter
