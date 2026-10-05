use std::collections::HashSet;

use thiserror::Error;

use super::*;
use crate::{
    ast::{
        self, AstExpr, AstExprKind, AstForcedTryExpr, AstFunctionDef, AstGuardConstraint, AstIdent,
        AstImplicitPathExpr, AstLetDecl, AstPath, AstPathExpr, AstRequireConstraint, AstStmtKind,
        AstSwitchCase, AstSwitchCaseItem, AstSwitchElseCase, AstTryCatchExpr,
    },
    defs::{self, FuncSig},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::{
        module::{ModuleId, ResolverError},
        res::{LocalId, OldId, ParamId, Res},
    },
};

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unable to resolve the type of {0}")]
    UnresolvableType(String, NodeId),
    #[error("types {0} and {1} are incompatible")]
    IncompatibleTypes(Ty, Ty, NodeId, NodeId),
    #[error("condition for if statement must be an int")]
    IfConditionNotValid(NodeId, Ty),
    #[error("it is not possible to perform operations on void values")]
    ExpressionOnVoid(ast::AstBinaryOperator, NodeId),
    #[error("it is not possible to perform operations on functions")]
    ExpressionOnFunc(Ty, ast::AstBinaryOperator, NodeId),
    #[error("cannot resolve name {0}")]
    CannotResolve(String, NodeId),
    #[error("cannot call non-function")]
    CannotCallNonFunction(NodeId),
    #[error("incorrect number of arguments")]
    IncorrectNumberOfArguments(NodeId, usize, usize),
    #[error("incorrect argument type (expected {0} for argument {2}, got {1})")]
    IncorrectArgumentType(Ty, Ty, usize, NodeId),
    #[error("cannot print func or void")]
    CannotPrintFuncOrVoid(NodeId),
    #[error("expected return value")]
    ExpectedReturn(NodeId, NodeId, Ty),
    #[error("did not expect return value")]
    DidNotExpectReturn(NodeId, Ty),
    #[error("dead code")]
    DeadCode(NodeId),
    #[error("not all branches return")]
    NotAllBranchesReturn(NodeId, NodeId, Ty),
    #[error("loop condition not int")]
    LoopConditionNotValid(NodeId, NodeId, Ty),
    #[error("break outside of a loop")]
    BreakOutsideLoop(NodeId),
    #[error("continue outside of a loop")]
    ContinueOutsideLoop(NodeId),
    #[error("cannot use `ret` inside an pre-constraint")]
    RetInPreConstraint(NodeId, NodeId),
    #[error("cannot use `old` inside a pre-constraint")]
    OldInPreConstraint(NodeId, NodeId),
    #[error("`old` inside constraints requires only one parameter")]
    MalformedOldInConstraint(NodeId, NodeId),
    #[error("condition in constraint is not an int")]
    ConstraintConditionNotValid(NodeId, Ty),
    #[error("implicit returns must be the last item in a block")]
    ImplicitReturnNotLast(NodeId),
    #[error("guard error does not match the thrown type (expected {1}, got {2})")]
    GuardErrorWrongType(NodeId, Ty, Ty),
    #[error("error in infallible function")]
    ErrorInInfallibleFunction(NodeId, NodeId),
    #[error("only enums may be thrown (got {1})")]
    InvalidThrowsType(NodeId, Ty),
    #[error("fallible types may not yet be stored")]
    FallibleTypesNotYetStored(NodeId),
    #[error("the given type can not have namespace members")]
    TypeHasNoNamespaceMembers(NodeId),
    #[error(
        "unable to imply variant with the given information, or the expected type is not an enum"
    )]
    CannotImplyVariant(NodeId),
    #[error("switch branches must be exhaustive")]
    NonExhaustiveSwitch(NodeId, Ty, Option<String>),
    #[error("switch targets must have a value")]
    SwitchTargetNotValue(NodeId, Ty),
    #[error("only one else block is permitted")]
    SeveralElseBranches(NodeId, Vec<NodeId>),
    #[error("the same variant is targeted several times")]
    DuplicateVariantInSwitch(NodeId, NodeId, Vec<NodeId>, String),
    #[error("fallthrough is only permitted inside non-else cases of switch statements")]
    FallthroughOutsideCase(NodeId),
    #[error("this fallthrough has nothing to fall through to")]
    FallthroughWithNothingToFallThroughTo(NodeId),
    #[error("the else branch of a switch must be the last branch")]
    ElseNotLast(NodeId),
    #[error("a function that throws must have its error handled directly with try! or try/catch")]
    DangerousUseThrowingFunction(NodeId, Ty, Ty),
    #[error("a fallible function may not contain a non-scalar return")]
    NonScalarFallible(NodeId, Ty),
    #[error("try used on a call that cannot throw")]
    TryOnNonThrowingCall(NodeId),
    #[error("the value of this block is discarded")]
    DiscardedBlockValue(NodeId, Ty),
    #[error("try/catch is not permitted inside constraints")]
    TryCatchInConstraint(NodeId, NodeId),
    #[error("the else block of a guard must not complete")]
    GuardElseMustDiverge(NodeId, NodeId),
    #[error("{1}")]
    PathResolutionError(NodeId, ResolverError),
    #[error("multiple entry points defined")]
    MultipleEntryPoints(NodeId, NodeId),
    #[error("cannot compare booleans")]
    CannotCompareBooleans(NodeId),
    #[error("cannot negate values of type {0}")]
    CannotNegate(Ty, NodeId),
    #[error("cannot perform unary operations on values of type {0}")]
    CannotUnaryOp(Ty, ast::AstUnaryOperator, NodeId),
    #[error("logical operators such as && and || require boolean operands")]
    LogicalOpOnNonBool(Ty, ast::AstBinaryOperator, NodeId),
    #[error("could not find intended target of use statement")]
    UseStatementResolutionError(NodeId),
    #[error("this expression never produces a value")]
    NeverValue(NodeId),
    #[error("control flow is not permitted inside constraints")]
    ControlFlowInConstraint(NodeId, NodeId),
    #[error("{1} is not mutable")]
    NotMutable(NodeId, String),
    #[error("cannot assign")]
    CannotAssign(NodeId, Res),
}

impl TypeError {
    pub fn as_code(&self) -> Option<usize> {
        use TypeError::*;
        match self {
            UnresolvableType(..) => Some(1001),
            IncompatibleTypes(..) => Some(1002),
            IfConditionNotValid(..) => Some(1003),
            ExpressionOnVoid(..) => Some(1004),
            ExpressionOnFunc(..) => Some(1005),
            CannotResolve(..) => Some(1006),
            CannotCallNonFunction(..) => Some(1007),
            IncorrectNumberOfArguments(..) => Some(1008),
            IncorrectArgumentType(..) => Some(1009),
            CannotPrintFuncOrVoid(..) => Some(1010),
            ExpectedReturn(..) => Some(1011),
            DidNotExpectReturn(..) => Some(1012),
            DeadCode(..) => Some(1013),
            NotAllBranchesReturn(..) => Some(1014),
            LoopConditionNotValid(..) => Some(1015),
            BreakOutsideLoop(..) => Some(1016),
            ContinueOutsideLoop(..) => Some(1017),
            RetInPreConstraint(..) => Some(1018),
            MalformedOldInConstraint(..) => Some(1019),
            ConstraintConditionNotValid(..) => Some(1020),
            OldInPreConstraint(..) => Some(1021),
            ImplicitReturnNotLast(..) => Some(1022),
            GuardErrorWrongType(..) => Some(1023),
            ErrorInInfallibleFunction(..) => Some(1024),
            InvalidThrowsType(..) => Some(1025),
            FallibleTypesNotYetStored(..) => Some(1026),
            TypeHasNoNamespaceMembers(..) => Some(1027),
            CannotImplyVariant(..) => Some(1028),
            NonExhaustiveSwitch(..) => Some(1029),
            SwitchTargetNotValue(..) => Some(1030),
            SeveralElseBranches(..) => Some(1031),
            DuplicateVariantInSwitch(..) => Some(1032),
            FallthroughOutsideCase(..) => Some(1033),
            FallthroughWithNothingToFallThroughTo(..) => Some(1034),
            ElseNotLast(..) => Some(1035),
            DangerousUseThrowingFunction(..) => Some(1036),
            NonScalarFallible(..) => Some(1037),
            TryOnNonThrowingCall(..) => Some(1038),
            DiscardedBlockValue(..) => Some(1039),
            TryCatchInConstraint(..) => Some(1040),
            GuardElseMustDiverge(..) => Some(1041),
            MultipleEntryPoints(..) => Some(1042),
            CannotCompareBooleans(..) => Some(1043),
            CannotNegate(..) => Some(1044),
            CannotUnaryOp(..) => Some(1045),
            LogicalOpOnNonBool(..) => Some(1046),
            UseStatementResolutionError(..) => Some(1047),
            NeverValue(..) => Some(1048),
            ControlFlowInConstraint(..) => Some(1049),
            NotMutable(..) => Some(1050),
            CannotAssign(..) => Some(1051),

            PathResolutionError(_, err) => Some(err.as_code()),
        }
    }
}

impl DiagnoseWith<NodeId> for TypeError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        use TypeError::*;
        let message = self.to_string();
        match self {
            UnresolvableType(_, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "item is here"),
                ]
            }
            IncompatibleTypes(left_ty, right_ty, left_node, right_node) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            left_node,
                            format!("this value has type {}", left_ty),
                        )
                        .with_label_from(
                            recorder,
                            right_node,
                            format!("but this value has type {}", right_ty),
                        ),
                ]
            }
            IfConditionNotValid(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("condition of type {} is here", ty),
                        ),
                ]
            }
            ExpressionOnVoid(op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to use {} on this value", op),
                        ),
                ]
            }
            ExpressionOnFunc(ty, op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to use {} on this function ({})", op, ty),
                        ),
                ]
            }
            CannotResolve(_, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "name is here"),
                ]
            }
            CannotCallNonFunction(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "attempted to call a non-function here",
                        ),
                ]
            }
            IncorrectNumberOfArguments(callee, args, correct_args) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            callee,
                            format!(
                                "attempted to call here with {} arguments, expected {}",
                                args, correct_args
                            ),
                        ),
                ]
            }
            IncorrectArgumentType(_, passed_ty, _, arg) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            arg,
                            format!("incorrect argument of type {} passed here", passed_ty),
                        ),
                ]
            }
            CannotPrintFuncOrVoid(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "attempted to print here"),
                ]
            }
            ExpectedReturn(node_id, ret_node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("expected return value of type {}", ty),
                        )
                        .with_label_from(recorder, ret_node_id, "return type defined here"),
                ]
            }
            DidNotExpectReturn(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("did not expect return value (found value of type {})", ty),
                        ),
                ]
            }
            DeadCode(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "this line (and following lines) will never be executed.",
                        ),
                ]
            }
            NotAllBranchesReturn(fn_node_id, return_node_id, return_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, fn_node_id, "in this function")
                        .with_label_from(
                            recorder,
                            return_node_id,
                            format!("expectation to return a {} defined here", return_ty),
                        ),
                ]
            }
            LoopConditionNotValid(node_id, condition_node_id, condition_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "loop here")
                        .with_label_from(
                            recorder,
                            condition_node_id,
                            format!(
                                "condition here expected to be an int, found a {}",
                                condition_ty
                            ),
                        ),
                ]
            }

            BreakOutsideLoop(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "break here"),
                ]
            }
            ContinueOutsideLoop(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "continue here"),
                ]
            }
            RetInPreConstraint(ret_node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, ret_node_id, "`ret` here")
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        ),
                ]
            }
            MalformedOldInConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "`old` here requires one parameter only",
                        )
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        )
                        .with_help(Some("`old` inside constraints refers to the language defined access of old values, not a defined function")),
                ]
            }
            ConstraintConditionNotValid(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("condition here must be an int, got {}", ty),
                        ),
                ]
            }
            OldInPreConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "`old` here is not allowed in pre-constraints",
                        )
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        ),
                ]
            }
            ImplicitReturnNotLast(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "implicit return here "),
                ]
            }
            GuardErrorWrongType(node_id, expected, _) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this function throws {expected}"),
                        ),
                ]
            }
            ErrorInInfallibleFunction(func_node_id, fallible_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, func_node_id, "infallible function defined here")
                        .with_label_from(recorder, fallible_node_id, "fallible operation used here")
                        .with_help(Some(
                            "are you missing a `throws X` notation on your function?",
                        )),
                ]
            }
            InvalidThrowsType(node_id, _) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "thrown type here"),
                ]
            }
            FallibleTypesNotYetStored(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "expression of fallible type here")
                        .with_help(Some("storing fallible types is not yet supported")),
                ]
            }
            TypeHasNoNamespaceMembers(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this expression attempts to resolve a member of an object that has none"),
                        )
                        .with_help(Some("did you mean to target the variant of an enum?")),
                ]
            }
            CannotImplyVariant(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this expression attempts to imply a variant of an enum, but the type is not an enum"),
                        )
                        .with_help(Some("only variants of enums can be implied with the ::variant syntax")),
                ]
            }
            NonExhaustiveSwitch(node_id, ty, missing_variant) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this switch statement is not exhaustive"),
                        )
                        .with_help(Some(if !ty.is_enum() {
                            "`switch`s on anything other than enums must have an `else` branch"
                                .to_string()
                        } else {
                            format!(
                                "you appear to be missing the `{}` variant",
                                missing_variant.as_deref().unwrap_or("")
                            )
                        })),
                ]
            }
            SwitchTargetNotValue(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this {ty} does not carry a value"),
                        )
                        .with_help(Some("are you trying to switch on a void value?")),
                ]
            }
            SeveralElseBranches(node_id, else_node_ids) => {
                let mut diag = Diagnostic::new(message)
                    .with_code(self.as_code())
                    .with_label_from(recorder, node_id, "first `else` branch here");

                for else_node_id in else_node_ids {
                    diag = diag.with_label_from(
                        recorder,
                        else_node_id,
                        "additional `else` branch here",
                    );
                }
                vec![diag]
            }
            DuplicateVariantInSwitch(enclosing_switch, original_case_id, cases, name) => {
                let mut diag = Diagnostic::new(message)
                    .with_code(self.as_code())
                    .with_label_from(
                        recorder,
                        enclosing_switch,
                        format!("matched `{name}` several times in this switch statement"),
                    )
                    .with_label_from(
                        recorder,
                        original_case_id,
                        format!("first matched `{name}` here"),
                    );

                for case_node_id in cases {
                    diag = diag.with_label_from(
                        recorder,
                        case_node_id,
                        format!("...matched `{name}` here again"),
                    );
                }

                diag = diag.with_help(Some(
                    "switch statements must match each variant at most once",
                ));
                vec![diag]
            }
            FallthroughOutsideCase(node_id) => {
                vec![Diagnostic::new(message)
                    .with_code(self.as_code())
                    .with_label_from(
                        recorder,
                        node_id,
                        "fallthrough is only permitted inside non-else cases of switch statements",
                    )]
            }
            FallthroughWithNothingToFallThroughTo(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "this fallthrough has nothing to fall through to",
                        ),
                ]
            }
            ElseNotLast(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "the else branch of a switch must be the last branch",
                        ),
                ]
            }
            DangerousUseThrowingFunction(node_id, returns_ty, throws_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "this call attempts to get the value of a throwing function without handling that error")
                        .with_help(Some(format!("this function returns {returns_ty} but throws {throws_ty}. you need to handle the latter.")))
                ]
            }
            TryOnNonThrowingCall(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "this call never throws")
                        .with_help(Some("remove the try")),
                ]
            }
            DiscardedBlockValue(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("this {ty} is the value of its block, which is unused"),
                        )
                        .with_help(Some(
                            "a trailing expression is the value of its own block. did you mean to `return` it, or add a `;`?",
                        )),
                ]
            }
            TryCatchInConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "try/catch here")
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        ),
                ]
            }
            ControlFlowInConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "control flow here")
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        ),
                ]
            }
            GuardElseMustDiverge(guard_node_id, else_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, guard_node_id, "in this guard")
                        .with_label_from(recorder, else_node_id, "this block can complete")
                        .with_help(Some(
                            "the else block runs when the condition fails, so it must return, throw, break or continue",
                        )),
                ]
            }
            NonScalarFallible(node_id, returns_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, format!("this function throws, but also wants to return a {}", returns_ty))
                        .with_help(Some("this may be a bug in the compiler as you generally can't manually specify non scalars."))
                ]
            }
            PathResolutionError(node_id, _) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "resolving this"),
                ]
            }
            MultipleEntryPoints(first_node_id, second_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, first_node_id, "entry point defined here")
                        .with_label_from(
                            recorder,
                            second_node_id,
                            "...another entry point defined here",
                        )
                        .with_help(Some("there can only be one entry point in a program")),
                ]
            }
            CannotCompareBooleans(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "cannot compare booleans here")
                        .with_help(Some("you cannot compare booleans with <, >, <=, >=, but you can use == and != to compare them")),
                ]
            }
            CannotNegate(_, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "attempted to negate here"),
                ]
            }
            CannotUnaryOp(_, op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "attempted to perform unary operation here",
                        )
                        .with_help(Some(format!("cannot perform {} on this type", op))),
                ]
            }
            LogicalOpOnNonBool(ty, op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to perform logical operation {} on a non-boolean type {}", op, ty),
                        )
                        .with_help(Some("logical operations such as && and || require boolean operands")),
                ]
            }
            UseStatementResolutionError(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "use statement here"),
                ]
            }
            NeverValue(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "this always returns, throws or jumps elsewhere, so has no value to use",
                        ),
                ]
            }
            NotMutable(node_id, name) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to mutate `{name}` here"),
                        )
                        .with_help(Some("did you forget to declare it as `var`?")),
                ]
            }
            CannotAssign(node_id, res) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to assign to {} here", res.as_name()),
                        )
                        .with_help(Some("did you mean to refer to a local variable?")),
                ]
            }
        }
    }
}

#[derive(Debug)]
pub struct BodyInfo {
    pub node_res: HashMap<ast::NodeId, Res>,
    pub local_tys: Vec<Ty>,
    pub local_mut: Vec<bool>,
    pub old_tys: Vec<Ty>,
    pub param_tys: Vec<Ty>,
    pub node_tys: HashMap<ast::NodeId, Ty>,

    /// will be used later for ARC mem management.
    pub scope_locals: HashMap<ScopeId, Vec<LocalId>>,
    /// what scopes are inside loops?
    pub scope_loops: HashMap<ScopeId, bool>,
    pub node_scopes: HashMap<ast::NodeId, ScopeId>,
    pub return_ty: Ty,
    pub throws_ty: Option<Ty>,
}

impl BodyInfo {
    pub fn new_with_return_ty(return_ty: Ty, throws_ty: Option<Ty>) -> Self {
        Self {
            node_res: HashMap::new(),
            local_tys: Vec::new(),
            local_mut: Vec::new(),
            old_tys: Vec::new(),
            param_tys: Vec::new(),
            node_tys: HashMap::new(),
            scope_locals: HashMap::new(),
            scope_loops: HashMap::new(),
            node_scopes: HashMap::new(),
            return_ty,
            throws_ty,
        }
    }

    pub fn node_ty(&self, node_id: ast::NodeId) -> Option<Ty> {
        self.node_tys.get(&node_id).cloned()
    }

    pub fn local_ty(&self, local_id: LocalId) -> Option<Ty> {
        self.local_tys.get(*local_id).cloned()
    }

    pub fn local_mut(&self, local_id: LocalId) -> Option<bool> {
        self.local_mut.get(*local_id).cloned()
    }

    pub fn param_ty(&self, param_id: ParamId) -> Option<Ty> {
        self.param_tys.get(*param_id).cloned()
    }

    pub fn node_res(&self, node_id: ast::NodeId) -> Option<Res> {
        self.node_res.get(&node_id).cloned()
    }

    fn set_node_ty(&mut self, node_id: ast::NodeId, ty: Ty) {
        self.node_tys.insert(node_id, ty);
    }

    fn set_node_res(&mut self, node_id: ast::NodeId, res: Res) {
        self.node_res.insert(node_id, res);
    }

    fn set_node_scope(&mut self, node_id: ast::NodeId, scope_id: ScopeId) {
        self.node_scopes.insert(node_id, scope_id);
    }

    pub fn node_scope(&self, node_id: ast::NodeId) -> Option<ScopeId> {
        self.node_scopes.get(&node_id).cloned()
    }

    fn set_scope_as_loop(&mut self, scope_id: ScopeId) {
        self.scope_loops.insert(scope_id, true);
    }

    pub fn is_scope_loop(&self, scope_id: ScopeId) -> bool {
        self.scope_loops.get(&scope_id).cloned().unwrap_or(false)
    }

    pub fn old_ty(&self, old_id: OldId) -> Option<Ty> {
        self.old_tys.get(*old_id).cloned()
    }
}

/// Exists only within a function and is discarded with the function.
enum ConstraintType {
    Pre(NodeId),
    Post(NodeId),
}

impl ConstraintType {
    pub fn node_id(&self) -> NodeId {
        match self {
            ConstraintType::Pre(node_id) | ConstraintType::Post(node_id) => *node_id,
        }
    }

    fn is_pre(&self) -> bool {
        matches!(self, ConstraintType::Pre(_))
    }

    fn is_post(&self) -> bool {
        matches!(self, ConstraintType::Post(_))
    }
}

enum SwitchCaseType {
    Case,
    Else,
}

impl From<&ast::AstSwitchCase> for SwitchCaseType {
    fn from(case: &ast::AstSwitchCase) -> Self {
        match case {
            ast::AstSwitchCase::Case(_) => SwitchCaseType::Case,
            ast::AstSwitchCase::Else(_) => SwitchCaseType::Else,
        }
    }
}

struct TypeckCtxt<'c> {
    next_id: usize,
    next_param_id: usize,
    next_scope_id: usize,
    next_old_id: usize,
    tcx: &'c mut TyCtxt,
    module_id: ModuleId,
    scopes: Vec<HashMap<String, Res>>,
    current_scope_ids: Vec<ScopeId>,
    /// Innermost loop, which `break` and `continue` target.
    current_loop: Option<NodeId>,
    loops_with_breaks: HashSet<NodeId>,
    inside_switch_case: Option<SwitchCaseType>,
    inside_fallthroughable_case: bool,
    inside_constraint: Option<ConstraintType>,
    permit_calls_to_throwing_funcs: bool,
    inferrable_tys: Vec<Option<Ty>>,
    // inside_try: bool,
    body: BodyInfo,
    errors: Vec<TypeError>,

    // Node IDs for diagnostics
    func_node_id: ast::NodeId,
    return_node_id: Option<ast::NodeId>,
    throws_node_id: Option<ast::NodeId>,
}

trait Reported {
    type Output;
    fn reported(self, tccx: &mut TypeckCtxt) -> Result<Self::Output, ()>;
}

impl<T> Reported for Result<T, TypeError> {
    type Output = T;

    fn reported(self, tccx: &mut TypeckCtxt) -> Result<T, ()> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(tccx.report(e)),
        }
    }
}

impl<T> Reported for Result<T, Vec<TypeError>> {
    type Output = T;

    fn reported(self, tccx: &mut TypeckCtxt) -> Result<T, ()> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(tccx.report_many(e)),
        }
    }
}

indexable_id!(pub ScopeId);
impl_next_id!(TypeckCtxt<'c>.next_id -> LocalId);
impl_next_id!(TypeckCtxt<'c>.next_param_id -> ParamId, next_param_id);
impl_next_id!(TypeckCtxt<'c>.next_scope_id -> ScopeId, next_scope_id);
impl_next_id!(TypeckCtxt<'c>.next_old_id -> OldId, next_old_id);
impl<'c> TypeckCtxt<'c> {
    pub fn new(tcx: &'c mut TyCtxt, module_id: ModuleId) -> Self {
        let return_ty = tcx.void_ty();
        Self {
            tcx,
            module_id,
            scopes: Vec::new(),
            body: BodyInfo::new_with_return_ty(return_ty, None),
            next_id: 0,
            next_param_id: 0,
            next_scope_id: 0,
            next_old_id: 0,
            errors: Vec::new(),
            current_loop: None,
            loops_with_breaks: HashSet::new(),
            inside_switch_case: None,
            inside_fallthroughable_case: false,
            inside_constraint: None,
            permit_calls_to_throwing_funcs: false,
            inferrable_tys: Vec::new(),
            func_node_id: ast::NodeId::new(0),
            current_scope_ids: Vec::new(),
            return_node_id: None,
            throws_node_id: None,
        }
    }

    fn is_inferrable_ty(&self) -> bool {
        self.inferrable_tys.last().is_some()
    }

    fn inferrable_ty(&self) -> Option<Ty> {
        self.inferrable_tys.last().cloned().unwrap_or(None)
    }

    fn begin_inferrable_ty(&mut self, ty: Option<Ty>) -> usize {
        self.inferrable_tys.push(ty);
        self.inferrable_tys.len() - 1
    }

    /// Less boilerplate to do
    ///
    /// ```no_run
    /// let idx = tccx.begin_inferrable_ty(Some(ty));
    /// ...
    /// tccx.end_inferrable_ty(idx);
    /// ```
    fn with_inferrable<T, F>(&mut self, inferrable_ty: &Ty, callback: F) -> T
    where
        F: FnOnce(&mut TypeckCtxt) -> T,
    {
        let idx = self.begin_inferrable_ty(Some(inferrable_ty.clone()));
        let res = callback(self);
        self.end_inferrable_ty(idx);
        res
    }

    fn end_inferrable_ty(&mut self, idx: usize) {
        debug_assert_eq!(
            idx,
            self.inferrable_tys.len() - 1,
            "end_inferrable_ty called out of order"
        );
        self.inferrable_tys.pop();
    }

    fn is_inside_case(&self) -> bool {
        matches!(self.inside_switch_case, Some(SwitchCaseType::Case))
    }

    fn new_with_func_sig(
        tcx: &'c mut TyCtxt,
        module_id: ModuleId,
        def: &ast::AstFunctionDef,
        sig: &defs::FuncSig,
    ) -> Self {
        let mut tccx = Self::new(tcx, module_id);
        tccx.open_scope(def.body.node_id);

        for (param, ty) in def.args.iter().zip(sig.param_tys.iter()) {
            tccx.declare_param(param.node_id, &param.name, ty.clone());
        }

        // Replace return type.
        tccx.body.return_ty = sig.return_ty.clone();
        tccx.body.throws_ty = sig.throws_ty.clone();
        tccx.func_node_id = def.node_id;
        tccx.return_node_id = def.return_node_id;
        tccx.throws_node_id = def.throws_node_id;
        tccx
    }

    fn report(&mut self, err: TypeError) {
        self.errors.push(err);
    }

    fn report_many(&mut self, errs: Vec<TypeError>) {
        self.errors.extend(errs);
    }

    fn declare_local(&mut self, node_id: NodeId, name: &str, ty: Ty, mutable: bool) -> LocalId {
        let local_id = self.next_id();
        self.body.local_tys.push(ty);
        self.body.local_mut.push(mutable);
        self.body.set_node_res(node_id, Res::Local(local_id));
        self.scopes
            .last_mut()
            .unwrap_or_else(|| bug!("tried to declare a local with no active scope"))
            .insert(name.to_string(), Res::Local(local_id));

        self.body
            .scope_locals
            .entry(
                self.current_scope_ids
                    .last()
                    .copied()
                    .unwrap_or_else(|| bug!("tried to declare a local with no active scope id")),
            )
            .or_default()
            .push(local_id);

        local_id
    }

    fn declare_param(&mut self, node_id: NodeId, name: &str, ty: Ty) -> ParamId {
        let param_id = self.next_param_id();
        self.body.param_tys.push(ty);
        self.body.set_node_res(node_id, Res::Param(param_id));
        self.scopes
            .last_mut()
            .unwrap_or_else(|| bug!("tried to declare a param with no active scope"))
            .insert(name.to_string(), Res::Param(param_id));
        param_id
    }

    fn resolve_local(&self, name: &str) -> Option<Res> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| {
                // prefer locals and params.
                s.get(name).cloned()
            })
            .or_else(|| self.tcx.resolve_single_path(name, self.module_id))
    }

    fn res_ty(&self, res: &Res) -> Option<Ty> {
        match res {
            Res::Local(local_id) => self.body.local_ty(*local_id),
            Res::Def(def_id) => {
                let def = self.tcx.defs.def(*def_id)?;
                match &def.kind {
                    defs::DefKind::Function(sig) => Some(self.ty(TyKind::Func(sig.clone()))),
                }
            }
            Res::ConstraintOld(res) => self.body.old_ty(*res),
            Res::ConstraintRet => Some(self.body.return_ty.clone()),
            Res::Param(param_id) => self.body.param_ty(*param_id),
            Res::Enum(enum_id) => Some(self.ty(TyKind::Enum(*enum_id))),
            Res::EnumVariant(enum_variant) => Some(self.ty(TyKind::Enum(enum_variant.enum_id()))),
            Res::Module(..) => None,
        }
    }

    // Just a helper/makes it nicer
    fn ty(&self, kind: TyKind) -> Ty {
        self.tcx.ty(kind)
    }

    fn open_scope(&mut self, owner_node_id: NodeId) -> ScopeId {
        self.scopes.push(HashMap::new());
        let scope_id = self.next_scope_id();
        self.current_scope_ids.push(scope_id);
        self.body.set_node_scope(owner_node_id, scope_id);

        scope_id
    }

    fn close_scope(&mut self) {
        self.scopes.pop();
        self.current_scope_ids.pop();
    }

    pub fn finish(self) -> Result<BodyInfo, Vec<TypeError>> {
        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(self.body)
    }
}

pub fn typeck_ast(
    tcx: &mut TyCtxt,
    program: &ast::AstModule,
) -> Result<HashMap<DefId, BodyInfo>, Vec<TypeError>> {
    typeck_module(tcx, program)
}

/// Walks the given AST and tries to apply types to every NodeId.
fn typeck_module(
    tcx: &mut TyCtxt,
    program: &ast::AstModule,
) -> Result<HashMap<DefId, BodyInfo>, Vec<TypeError>> {
    let mut results = HashMap::new();
    let mut errors = Vec::new();
    for ast_def in program.defs.iter() {
        match ast_def {
            ast::AstDef::Function(ast_func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(ast_func_def.node_id)
                    .unwrap_or_else(|| {
                        bug!(
                        "a function definition was unable to be resolved to a def_id during typeck"
                    )
                    });
                let def = tcx
                    .defs
                    .def(def_id)
                    .unwrap_or_else(|| {
                        bug!("a resolved def_id has no associated def during typeck")
                    })
                    .clone();
                match def {
                    defs::Def {
                        kind: defs::DefKind::Function(sig),
                    } => {
                        let mut tccx = TypeckCtxt::new_with_func_sig(
                            tcx,
                            tcx.mcx.get_module_id(program.node_id).unwrap_or_else(|| {
                                bug!("unable to resolve module id of module in typeck")
                            }),
                            ast_func_def,
                            &sig,
                        );
                        let _ =
                            typeck_mark_entry_point(&mut tccx, ast_func_def).reported(&mut tccx);
                        let _ = typeck_sig(&mut tccx, &sig).reported(&mut tccx);
                        let _ = typeck_pre_constraint(&mut tccx, &ast_func_def.constraints);
                        // The body's scope is already open (from new_with_func_sig) as
                        // it also holds the params, hence not using typeck_block.
                        let body = &ast_func_def.body;
                        let body_ty = typeck_block_stmts(&mut tccx, body, Some(&sig.return_ty));
                        // A block-like without a value is just the last statement (not a returning
                        // expression, like implicitreturn) so we should mark it as a missing return
                        // and not a mismatched type.
                        let tail = body.tail().filter(|tail| {
                            !(tail.is_block_like()
                                && body_ty.as_ref().is_ok_and(|ty| *ty.kind() == TyKind::Void))
                        });
                        if let (Ok(ty), Some(tail)) = (body_ty, tail) {
                            let tail_node_id = value_node_id(&tccx.body, tail);
                            let _ =
                                typeck_tail_return(&mut tccx, tail_node_id, ty).reported(&mut tccx);
                        }
                        let _ = typeck_post_constraint(&mut tccx, &ast_func_def.constraints);

                        // will eventually be togglable, but for now we do this always
                        match (
                            sig.return_ty.kind(),
                            block_diverges(body, &tccx.loops_with_breaks),
                        ) {
                            (_, Diverges(true, dead_code)) => {
                                errors.extend(
                                    dead_code
                                        .iter()
                                        .map(|node_id| TypeError::DeadCode(*node_id)),
                                );
                            }
                            (TyKind::Void, _) => {}
                            // The body's value is the return value.
                            _ if tail.is_some() => {}
                            _ => {
                                errors.push(TypeError::NotAllBranchesReturn(
                                    ast_func_def.node_id,
                                    ast_func_def.return_node_id.unwrap_or_else(|| {
                                        bug!("a non-void function has no return type node id")
                                    }),
                                    tccx.body.return_ty.clone(),
                                ));
                            }
                        };

                        match tccx.finish() {
                            Ok(res) => {
                                results.insert(def_id, res);
                            }
                            Err(err) => {
                                errors.extend(err);
                            }
                        };
                    }
                }
            }
        }
    }

    for module in program.mods.iter() {
        match typeck_module(tcx, module) {
            Ok(module_results) => {
                results.extend(module_results);
            }
            Err(err) => {
                errors.extend(err);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(results)
}

fn typeck_mark_entry_point(tccx: &mut TypeckCtxt, def: &AstFunctionDef) -> Result<(), TypeError> {
    let module = tccx
        .tcx
        .mcx
        .get_module(tccx.module_id)
        .unwrap_or_else(|| bug!("cannot find module when checking for entry point"));

    if def.name != "main" || module.parent.is_some() {
        return Ok(());
    }

    match tccx.tcx.entry_point {
        None => {
            tccx.tcx.entry_point = Some(
                tccx.tcx
                    .defs
                    .resolve_def_id_for_node_id(def.node_id)
                    .unwrap_or_else(|| bug!("cannot find def id for node id")),
            );
            Ok(())
        }
        Some(existing_entry_point) => {
            let original_node_id = tccx
                .tcx
                .defs
                .resolve_node_id_for_def_id(existing_entry_point)
                .unwrap_or_else(|| bug!("cannot find node id for def id"));

            Err(TypeError::MultipleEntryPoints(
                original_node_id,
                def.node_id,
            ))
        }
    }
}

fn typeck_sig(tccx: &mut TypeckCtxt, sig: &FuncSig) -> Result<(), TypeError> {
    // thrown values travel in a single integer slot, so only enums for now.
    if let Some(throws_ty) = &sig.throws_ty
        && !throws_ty.is_enum()
    {
        return Err(TypeError::InvalidThrowsType(
            tccx.throws_node_id
                .unwrap_or_else(|| bug!("a function that throws has no throws node id")),
            throws_ty.clone(),
        ));
    }

    Ok(())
}

fn typeck_pre_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            ast::AstConstraint::Require(AstRequireConstraint {
                node_id, condition, ..
            }) => {
                typeck_constraint_condition(tccx, ConstraintType::Pre(*node_id), condition)
                    .reported(tccx)?;
            }
            ast::AstConstraint::Guard(AstGuardConstraint {
                node_id,
                condition,
                error,
            }) => {
                typeck_guard_constraint_error(tccx, *node_id, error).reported(tccx)?;
                typeck_constraint_condition(tccx, ConstraintType::Pre(*node_id), condition)
                    .reported(tccx)?;
            }
            // Happens in post_constraint
            ast::AstConstraint::Ensure(..) => {}
        }
    }

    Ok(())
}

fn typeck_guard_constraint_error(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    error: &AstExpr,
) -> Result<(), TypeError> {
    let throws_ty = typeck_fallible(tccx, node_id)?;

    let ity = tccx.begin_inferrable_ty(Some(throws_ty.clone()));
    let error_ty = typeck_expr(tccx, error);
    tccx.end_inferrable_ty(ity);
    let error_ty = error_ty?;

    if error_ty != throws_ty {
        return Err(TypeError::GuardErrorWrongType(
            error.node_id(),
            throws_ty,
            error_ty,
        ));
    }

    Ok(())
}

fn typeck_constraint_condition(
    tccx: &mut TypeckCtxt,
    type_: ConstraintType,
    condition: &ast::AstExpr,
) -> Result<Ty, TypeError> {
    tccx.inside_constraint = Some(type_);
    let ty = typeck_expr(tccx, condition);
    tccx.inside_constraint = None;
    let ty = ty?;
    if !ty.is_bool() {
        return Err(TypeError::ConstraintConditionNotValid(
            condition.node_id(),
            ty,
        ));
    }
    Ok(ty)
}

fn typeck_post_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            // pre_constraint
            ast::AstConstraint::Require(..) | ast::AstConstraint::Guard(..) => {}
            ast::AstConstraint::Ensure(ens_cstrt) => {
                typeck_constraint_condition(
                    tccx,
                    ConstraintType::Post(ens_cstrt.node_id),
                    &ens_cstrt.condition,
                )
                .reported(tccx)?;
            }
        }
    }
    Ok(())
}

fn typeck_fallible(tccx: &mut TypeckCtxt, node_id: NodeId) -> Result<Ty, TypeError> {
    match tccx.body.throws_ty {
        None => Err(TypeError::ErrorInInfallibleFunction(
            tccx.func_node_id,
            node_id,
        )),
        Some(ref throws_ty) => Ok(throws_ty.clone()),
    }
}

fn typeck_stmt(tccx: &mut TypeckCtxt, stmt: &ast::AstStmt) -> Result<(), ()> {
    match &stmt.kind {
        AstStmtKind::Expr(expr) => {
            typeck_expr(tccx, expr).reported(tccx)?;
        }
        AstStmtKind::BlockLike(expr) => typeck_block_like_stmt(tccx, expr)?,
        AstStmtKind::Decl(decl) => {
            typeck_decl(tccx, decl).reported(tccx)?;
        }
        AstStmtKind::Assign(ass) => {
            let target = tccx
                .resolve_local(&ass.name.text)
                .ok_or_else(|| TypeError::CannotResolve(ass.name.text.clone(), ass.name.node_id))
                .reported(tccx)?;

            typeck_assignment_local_mut(tccx, &target, ass).reported(tccx)?;
            tccx.body.set_node_res(ass.node_id, target);
            let ty = typeck_value_expr(tccx, &ass.expr).reported(tccx)?;
            if ty.is_fallible() {
                return Err(TypeError::FallibleTypesNotYetStored(ass.expr.node_id()))
                    .reported(tccx);
            }
        }
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            let ty = typeck_value_expr(tccx, &stmt.expr).reported(tccx)?;

            // Cannot print voids and funcs
            if matches!(ty.kind(), TyKind::Func(..) | TyKind::Void) {
                return Err(TypeError::CannotPrintFuncOrVoid(stmt.node_id)).reported(tccx);
            }
        }
        // typeck_block_stmts handles the tail itself, so any here are misplaced.
        AstStmtKind::ImplicitReturn(expr) => {
            return Err(TypeError::ImplicitReturnNotLast(expr.node_id())).reported(tccx);
        }
        AstStmtKind::Guard(guard_stmt) => typeck_guard_stmt(tccx, guard_stmt)?,
        AstStmtKind::Require(require) => {
            typeck_constraint_condition(
                tccx,
                ConstraintType::Pre(require.node_id),
                &require.condition,
            )
            .reported(tccx)?;
        }
        AstStmtKind::Use(use_stmt) => {
            typeck_use_stmt(tccx, use_stmt).reported(tccx)?;
        }
    };

    Ok(())
}

/// A block-like without a `;` has nowhere to send its value, so may not have
/// one.
fn typeck_block_like_stmt(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<(), ()> {
    let void_ty = tccx.tcx.void_ty();
    let ty = tccx
        .with_inferrable(&void_ty, |tccx| typeck_expr(tccx, expr))
        .reported(tccx)?;
    if ty.has_value() {
        return Err(TypeError::DiscardedBlockValue(
            value_node_id(&tccx.body, expr),
            ty,
        ))
        .reported(tccx);
    }

    Ok(())
}

fn typeck_assignment_local_mut(
    tccx: &mut TypeckCtxt,
    target: &Res,
    ass: &ast::AstAssignStmt,
) -> Result<(), TypeError> {
    match target {
        Res::Local(local_id) => {
            if !tccx.body.local_mut(*local_id).unwrap_or(false) {
                return Err(TypeError::NotMutable(ass.node_id, ass.name.text.clone()));
            }
        }
        _ => {
            return Err(TypeError::CannotAssign(ass.node_id, target.clone()));
        }
    }

    Ok(())
}

/// Typechecks an expression whose value is used, which a `never` one lacks.
fn typeck_value_expr(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<Ty, TypeError> {
    let errors = tccx.errors.len();
    let ty = typeck_expr(tccx, expr)?;
    // An expression with errors is `never` to avoid more (see [recover]).
    if *ty.kind() == TyKind::Never && tccx.errors.len() == errors {
        return Err(TypeError::NeverValue(expr.node_id()));
    }

    Ok(ty)
}

fn typeck_use_stmt(
    tccx: &mut TypeckCtxt,
    use_stmt: &ast::AstUseStmt,
) -> Result<(), Vec<TypeError>> {
    let mut errors = Vec::new();
    for entry in &use_stmt.entries {
        let path_to_resolve = entry.path.clone();
        let name = entry.name().to_owned();
        match tccx.tcx.resolve_path(path_to_resolve, tccx.module_id) {
            Ok(Some(res)) => {
                tccx.body.set_node_res(entry.node_id, res.clone());
                tccx.scopes
                    .last_mut()
                    .unwrap_or_else(|| bug!("tried to declare a use with no active scope"))
                    .insert(name, res);
            }
            Ok(None) => {
                errors.push(TypeError::UseStatementResolutionError(entry.node_id));
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn typeck_guard_stmt(tccx: &mut TypeckCtxt, guard_stmt: &ast::AstGuardStmt) -> Result<(), ()> {
    let ty = typeck_expr(tccx, &guard_stmt.condition).reported(tccx)?;
    if !ty.is_bool() {
        return Err(TypeError::ConstraintConditionNotValid(
            guard_stmt.condition.node_id(),
            ty,
        ))
        .reported(tccx);
    }

    // Code after the guard relies on the condition holding.
    let void_ty = tccx.tcx.void_ty();
    let else_ty = typeck_block(tccx, &guard_stmt.else_, false, Some(&void_ty))?;
    if !matches!(else_ty.kind(), TyKind::Never) {
        return Err(TypeError::GuardElseMustDiverge(
            guard_stmt.node_id,
            guard_stmt.else_.node_id,
        ))
        .reported(tccx);
    }

    Ok(())
}

fn typeck_throw(tccx: &mut TypeckCtxt, throw_stmt: &ast::AstThrowStmt) -> Result<(), TypeError> {
    let throws_ty = typeck_fallible(tccx, throw_stmt.node_id)?;

    let expr_ty = tccx.with_inferrable(&throws_ty, |tccx| {
        typeck_expr(tccx, throw_stmt.expr.as_ref())
    })?;

    if expr_ty != throws_ty {
        return Err(TypeError::IncompatibleTypes(
            expr_ty,
            throws_ty.clone(),
            throw_stmt.expr.node_id(),
            throw_stmt.node_id,
        ));
    }

    Ok(())
}

fn typeck_fallthrough(
    tccx: &mut TypeckCtxt,
    fallthrough_stmt: &ast::AstFallthroughStmt,
) -> Result<(), TypeError> {
    if !tccx.is_inside_case() {
        return Err(TypeError::FallthroughOutsideCase(fallthrough_stmt.node_id));
    }
    if !tccx.inside_fallthroughable_case {
        return Err(TypeError::FallthroughWithNothingToFallThroughTo(
            fallthrough_stmt.node_id,
        ));
    }

    Ok(())
}

/// Returns the type of the cases' values, which must agree.
fn typeck_switch(tccx: &mut TypeckCtxt, switch_stmt: &ast::AstSwitchStmt) -> Result<Ty, ()> {
    let switch_ty = typeck_expr(tccx, &switch_stmt.expr).reported(tccx)?;
    let expected = tccx.inferrable_ty();
    // Every case so far has diverged, so has no value to agree with.
    let mut value: Option<(Ty, NodeId)> = None;

    let cases_len = switch_stmt.cases.len();
    for (i, case) in switch_stmt.cases.iter().enumerate() {
        let old_switch_case = tccx.inside_switch_case.take();
        let old_fallthroughable_case = tccx.inside_fallthroughable_case;
        tccx.inside_switch_case = Some(case.into());
        tccx.inside_fallthroughable_case =
            i < cases_len - 1 && matches!(case, AstSwitchCase::Case(_));
        if let AstSwitchCase::Case(case) = case {
            let case_ty = tccx.with_inferrable(&switch_ty, |tccx| {
                typeck_expr(tccx, &case.expr).reported(tccx)
            });
            let case_ty = case_ty?;

            if switch_ty != case_ty {
                return Err(TypeError::IncompatibleTypes(
                    case_ty,
                    switch_ty.clone(),
                    case.expr.node_id(),
                    switch_stmt.node_id,
                ))
                .reported(tccx);
            }
        }

        let body_ty = typeck_block(tccx, case.body(), true, expected.as_ref());
        tccx.inside_switch_case = old_switch_case;
        tccx.inside_fallthroughable_case = old_fallthroughable_case;

        let body_node_id = block_value_node_id(&tccx.body, case.body());
        match (body_ty?, &value) {
            (body_ty, _) if *body_ty.kind() == TyKind::Never => {}
            (body_ty, None) => value = Some((body_ty, body_node_id)),
            (body_ty, Some((value_ty, value_node_id))) if body_ty != *value_ty => {
                return Err(TypeError::IncompatibleTypes(
                    body_ty,
                    value_ty.clone(),
                    body_node_id,
                    *value_node_id,
                ))
                .reported(tccx);
            }
            _ => {}
        }
    }

    // do we have several elses? or is the else not last
    let mut else_node_ids = Vec::new();
    for (i, case) in switch_stmt.cases.iter().enumerate() {
        if let AstSwitchCase::Else(else_case) = case {
            if i != cases_len - 1 {
                return Err(TypeError::ElseNotLast(else_case.node_id)).reported(tccx);
            }
            else_node_ids.push(else_case.node_id);
        }
    }

    if else_node_ids.len() > 1 {
        let (start, others) = else_node_ids
            .split_first()
            .unwrap_or_else(|| bug!("else_node_ids was checked to be non-empty, but was empty"));
        return Err(TypeError::SeveralElseBranches(*start, others.to_vec())).reported(tccx);
    }

    // We must have an else if we don't match all variants of the enum, or the ty
    // is not an enum.
    if !switch_ty.is_enum() && !switch_stmt.has_else() {
        return Err(TypeError::NonExhaustiveSwitch(
            switch_stmt.node_id,
            switch_ty.clone(),
            None,
        ))
        .reported(tccx);
    }

    if switch_ty.is_enum() {
        // collect all variants of the enum and check that they are all covered
        // by the cases.
        let TyKind::Enum(enum_id) = switch_ty.kind() else {
            unreachable!("switch_ty.is_enum() should guarantee this");
        };
        let enum_ = tccx
            .tcx
            .enums
            .get_enum(*enum_id)
            .unwrap_or_else(|| bug!("an enum id from a type has no associated enum"));

        let case_node_ids = switch_stmt
            .cases()
            .map(|case| case.expr.node_id())
            .collect::<Vec<_>>();

        let found_variants = switch_stmt
            .cases()
            .map(|case| {
                // make sure we cover all variants
                let Res::EnumVariant(enum_variant) = tccx
                    .body
                    .node_res(case.expr.node_id())
                    .unwrap_or_else(|| bug!("a switch case on an enum has no resolution"))
                else {
                    unreachable!("case of enum must be enum variant")
                };
                enum_variant
            })
            .collect::<Vec<_>>();

        let real_variants = enum_.variants().collect::<Vec<_>>();
        for real_variant in real_variants.iter() {
            if !found_variants.contains(real_variant) && !switch_stmt.has_else() {
                return Err(TypeError::NonExhaustiveSwitch(
                    switch_stmt.node_id,
                    switch_ty.clone(),
                    enum_.name_of_variant(*real_variant).map(|s| s.to_string()),
                ))
                .reported(tccx);
            }

            // also make sure we have no duplicates
            let matches = found_variants
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    if v == real_variant {
                        Some((v, case_node_ids[i]))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if matches.len() > 1 {
                let (first, others) = matches
                    .split_first()
                    .unwrap_or_else(|| bug!("matches was checked to be non-empty, but was empty"));
                return Err(TypeError::DuplicateVariantInSwitch(
                    switch_stmt.node_id,
                    first.1,
                    others.into_iter().map(|(_, node_id)| *node_id).collect(),
                    enum_
                        .name_of_variant(*first.0)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                ))
                .reported(tccx)?;
            }
        }
    }

    Ok(value.map_or_else(|| tccx.ty(TyKind::Never), |(ty, _)| ty))
}

fn typeck_gen_loop(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    body: &ast::AstBlockStmt,
    condition: Option<&AstExpr>,
) -> Result<(), ()> {
    if let Some(condition) = condition {
        // make sure the condition resolves a int (current placeholder for booleans)
        let ty = typeck_expr(tccx, condition).reported(tccx)?;
        if !ty.is_bool() {
            return Err(TypeError::LoopConditionNotValid(
                node_id,
                condition.node_id(),
                ty,
            ))
            .reported(tccx)?;
        }
    }

    let outer_loop = tccx.current_loop.replace(node_id);
    let res = typeck_stmt_block(tccx, body, true);
    tccx.current_loop = outer_loop;
    res?;

    Ok(())
}

fn typeck_break(tccx: &mut TypeckCtxt, break_stmt: &ast::AstBreakStmt) -> Result<(), TypeError> {
    let Some(loop_node_id) = tccx.current_loop else {
        return Err(TypeError::BreakOutsideLoop(break_stmt.node_id));
    };
    tccx.loops_with_breaks.insert(loop_node_id);

    Ok(())
}

fn typeck_continue(
    tccx: &mut TypeckCtxt,
    continue_stmt: &ast::AstContinueStmt,
) -> Result<(), TypeError> {
    if tccx.current_loop.is_none() {
        return Err(TypeError::ContinueOutsideLoop(continue_stmt.node_id));
    }

    Ok(())
}

/// Checks the type of a function body's tail against the return type.
fn typeck_tail_return(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    return_ty: Ty,
) -> Result<(), TypeError> {
    match (return_ty.kind(), tccx.body.return_ty.kind()) {
        (TyKind::Void, TyKind::Void) | (TyKind::Never, _) => Ok(()),
        (TyKind::Void, _) => Err(TypeError::ExpectedReturn(
            node_id,
            tccx.return_node_id
                .unwrap_or_else(|| bug!("a non-void function has no return type node id")),
            tccx.body.return_ty.clone(),
        )),
        (_, TyKind::Void) => Err(TypeError::DidNotExpectReturn(node_id, return_ty.clone())),
        (ty, expected) if ty != expected => Err(TypeError::IncompatibleTypes(
            return_ty.clone(),
            tccx.body.return_ty.clone(),
            node_id,
            tccx.return_node_id.unwrap_or_else(|| {
                bug!("a function with a return type has no return type node id")
            }),
        )),
        _ => Ok(()),
    }
}

fn typeck_return(tccx: &mut TypeckCtxt, return_stmt: &ast::AstReturnStmt) -> Result<(), TypeError> {
    let return_ty = tccx.body.return_ty.clone();
    let return_ty = return_stmt
        .expr
        .as_ref()
        .map(|expr| tccx.with_inferrable(&return_ty, |tccx| typeck_expr(tccx, expr)))
        .transpose()?; // returns always follow success path.

    match (&return_ty, tccx.body.return_ty.kind()) {
        // Returns but didn't expect return (non-void return in void context)
        (Some(ty), TyKind::Void) if ty.kind() != &TyKind::Void => Err(
            TypeError::DidNotExpectReturn(return_stmt.node_id, ty.clone()),
        ),
        // Returns wrong type
        (Some(ty), expected) if ty.kind() != expected => Err(TypeError::IncompatibleTypes(
            ty.clone(),
            tccx.body.return_ty.clone(),
            return_stmt.node_id,
            tccx.return_node_id.unwrap_or_else(|| {
                bug!("a function with a return type has no return type node id")
            }),
        )),
        // Returns and expects return or no return and expects void
        // This also covers returning (but returning void from somewhere else)
        // or not returning at all in void context
        (Some(_), _) | (None, TyKind::Void) => Ok(()),
        // Did not return, expected return (see above for None, TyKind::Void)
        // case handled.
        (None, _) => Err(TypeError::ExpectedReturn(
            return_stmt.node_id,
            tccx.return_node_id
                .unwrap_or_else(|| bug!("a non-void function has no return type node id")),
            tccx.body.return_ty.clone(),
        )),
    }
}

/// Returns the type of the branches' values, which must agree.
fn typeck_if(tccx: &mut TypeckCtxt, if_expr: &ast::AstIfStmt) -> Result<Ty, ()> {
    let res = typeck_expr(tccx, &if_expr.cond).reported(tccx)?;
    if !res.is_bool() {
        return Err(TypeError::IfConditionNotValid(if_expr.cond.node_id(), res)).reported(tccx);
    }

    // Without an else there is no value when the condition fails.
    let Some(else_) = &if_expr.else_ else {
        typeck_stmt_block(tccx, &if_expr.then, false)?;
        return Ok(tccx.tcx.void_ty());
    };

    let expected = tccx.inferrable_ty();
    let then_ty = typeck_block(tccx, &if_expr.then, false, expected.as_ref());
    let (else_ty, else_node_id) = match else_ {
        ast::AstElseBranch::Block(block) => (
            typeck_block(tccx, block, false, expected.as_ref()),
            block_value_node_id(&tccx.body, block),
        ),
        ast::AstElseBranch::If(nested) => {
            let ty = typeck_if(tccx, nested);
            (ty, if_value_node_id(&tccx.body, nested))
        }
    };
    let (then_ty, else_ty) = (then_ty?, else_ty?);

    // A branch that diverges doesn't have to conform (as it doesn't yield)
    if else_ty.coerces_to(&then_ty) {
        Ok(then_ty)
    } else if then_ty.coerces_to(&else_ty) {
        Ok(else_ty)
    } else {
        Err(TypeError::IncompatibleTypes(
            else_ty,
            then_ty,
            else_node_id,
            block_value_node_id(&tccx.body, &if_expr.then),
        ))
        .reported(tccx)
    }
}

/// Returns the type the block evaluates to (see [typeck_block_stmts]).
fn typeck_block(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    is_loop: bool,
    expected: Option<&Ty>,
) -> Result<Ty, ()> {
    let scope_id = tccx.open_scope(block.node_id);
    if is_loop {
        tccx.body.set_scope_as_loop(scope_id);
    }
    let ty = typeck_block_stmts(tccx, block, expected);
    tccx.close_scope();

    // Kept so diagnostics can find which branch a value came from.
    if let Ok(ty) = &ty {
        tccx.body.set_node_ty(block.node_id, ty.clone());
    }
    ty
}

/// The node to point at for an expression's value. A block-like's comes from
/// a tail, so look through to the first one that has a value.
fn value_node_id(body: &BodyInfo, expr: &ast::AstExpr) -> NodeId {
    match &expr.kind {
        AstExprKind::Block(block) => block_value_node_id(body, block),
        AstExprKind::If(if_expr) => if_value_node_id(body, if_expr),
        AstExprKind::Switch(switch_expr) => switch_expr
            .cases
            .iter()
            .map(|case| case.body())
            .find(|block| block_has_value(body, block))
            .map_or(expr.node_id(), |block| block_value_node_id(body, block)),
        _ => expr.node_id(),
    }
}

fn block_value_node_id(body: &BodyInfo, block: &ast::AstBlockStmt) -> NodeId {
    block
        .tail()
        .map_or(block.node_id, |tail| value_node_id(body, tail))
}

/// Skips branches without a value (e.g. `{ return 1; }`) as they aren't
/// where it came from.
fn if_value_node_id(body: &BodyInfo, if_expr: &ast::AstIfStmt) -> NodeId {
    match &if_expr.else_ {
        _ if block_has_value(body, &if_expr.then) => block_value_node_id(body, &if_expr.then),
        Some(ast::AstElseBranch::Block(block)) if block_has_value(body, block) => {
            block_value_node_id(body, block)
        }
        Some(ast::AstElseBranch::If(nested)) => if_value_node_id(body, nested),
        _ => block_value_node_id(body, &if_expr.then),
    }
}

fn block_has_value(body: &BodyInfo, block: &ast::AstBlockStmt) -> bool {
    body.node_ty(block.node_id).is_some_and(|ty| ty.has_value())
}

/// A block in statement position has nowhere to send its value, so it may not
/// have one.
fn typeck_stmt_block(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    is_loop: bool,
) -> Result<(), ()> {
    let void_ty = tccx.tcx.void_ty();
    let ty = typeck_block(tccx, block, is_loop, Some(&void_ty))?;
    if ty.has_value() {
        let tail = block
            .stmts
            .last()
            .unwrap_or_else(|| bug!("a block with a value has no statements"));
        return Err(TypeError::DiscardedBlockValue(tail.inner_node_id(), ty)).reported(tccx);
    }

    Ok(())
}

/// Typechecks a block's statements in the current scope. The block evaluates
/// to its tail if it has one, otherwise `never` if it diverges, or `void`.
fn typeck_block_stmts(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    expected: Option<&Ty>,
) -> Result<Ty, ()> {
    for (i, stmt) in block.stmts.iter().enumerate() {
        if i + 1 == block.stmts.len()
            && let AstStmtKind::ImplicitReturn(tail) = &stmt.kind
        {
            let ity = tccx.begin_inferrable_ty(expected.cloned());
            let ty = typeck_expr(tccx, tail);
            tccx.end_inferrable_ty(ity);
            return ty.reported(tccx);
        }
        let _ = typeck_stmt(tccx, stmt);
    }

    Ok(match block_diverges(block, &tccx.loops_with_breaks) {
        Diverges(true, _) => tccx.ty(TyKind::Never),
        Diverges(false, _) => tccx.tcx.void_ty(),
    })
}

fn typeck_decl(tccx: &mut TypeckCtxt, decl: &ast::AstDecl) -> Result<(), TypeError> {
    match &decl.kind {
        ast::AstDeclKind::Let(AstLetDecl {
            expr,
            mutable,
            name: AstIdent {
                text: name,
                node_id,
            },
        }) => {
            let ty = typeck_value_expr(tccx, expr)?;
            if ty.is_fallible() {
                return Err(TypeError::FallibleTypesNotYetStored(*node_id));
            }
            tccx.declare_local(*node_id, name, ty, *mutable);
        }
    };

    Ok(())
}

fn typeck_expr(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<Ty, TypeError> {
    let constraint_expr_ty = typeck_constraint_expr(tccx, expr)?;

    // probably a prettier way of doing this, surely?
    let ty = match (constraint_expr_ty, &expr.kind) {
        (Some(ty), _) => Ok(ty),
        (
            None,
            ast::AstExprKind::Ident(ast::AstIdent {
                text: name,
                node_id,
            }),
        ) => {
            let res = tccx
                .resolve_local(name)
                .ok_or_else(|| TypeError::CannotResolve(name.clone(), *node_id))?;

            let ty = tccx
                .res_ty(&res)
                .ok_or_else(|| TypeError::UnresolvableType(name.clone(), *node_id))?;

            // Also store in node_res for codegen.
            tccx.body.set_node_res(*node_id, res);

            Ok(ty)
        }
        (None, ast::AstExprKind::Literal(lit)) => match lit.value {
            ast::AstLiteralKind::Int(_) => Ok(tccx.ty(TyKind::Int)),
            ast::AstLiteralKind::Float(_) => Ok(tccx.ty(TyKind::Float)),
            ast::AstLiteralKind::Bool(_) => Ok(tccx.ty(TyKind::Bool)),
        },

        (None, ast::AstExprKind::Unary(un_expr)) => typeck_unary_expr(tccx, &un_expr),
        (None, ast::AstExprKind::Binary(bin_expr)) => typeck_binary_expr(tccx, &bin_expr),
        (None, ast::AstExprKind::Call(call_expr)) => typeck_call_expr(tccx, &call_expr),
        (None, ast::AstExprKind::Path(path_expr)) => typeck_path_expr(tccx, path_expr),
        (None, ast::AstExprKind::ImplicitPath(impl_path_expr)) => {
            typeck_implicit_path_expr(tccx, impl_path_expr)
        }
        (None, ast::AstExprKind::ForcedTry(forced_try_expr)) => {
            typeck_forced_try_expr(tccx, forced_try_expr)
        }
        (None, ast::AstExprKind::TryCatch(try_catch_expr)) => {
            typeck_try_catch_expr(tccx, try_catch_expr)
        }
        (None, ast::AstExprKind::Block(block)) => {
            let expected = tccx.inferrable_ty();
            Ok(recover(tccx, |tccx| {
                typeck_block(tccx, block, false, expected.as_ref())
            }))
        }
        (None, ast::AstExprKind::If(if_expr)) => Ok(recover(tccx, |tccx| typeck_if(tccx, if_expr))),
        (None, ast::AstExprKind::Switch(switch_expr)) => {
            Ok(recover(tccx, |tccx| typeck_switch(tccx, switch_expr)))
        }
        (None, ast::AstExprKind::Loop(loop_expr)) => {
            let _ = typeck_gen_loop(tccx, loop_expr.node_id, &loop_expr.body, None);
            // Only a `break` ends a `loop`, whatever its body does.
            Ok(if tccx.loops_with_breaks.contains(&loop_expr.node_id) {
                tccx.tcx.void_ty()
            } else {
                tccx.ty(TyKind::Never)
            })
        }
        (None, ast::AstExprKind::While(while_expr)) => {
            let _ = typeck_gen_loop(
                tccx,
                while_expr.node_id,
                &while_expr.body,
                Some(&while_expr.cond),
            );
            Ok(tccx.tcx.void_ty())
        }
        (None, ast::AstExprKind::Return(return_expr)) => {
            typeck_return(tccx, return_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Throw(throw_expr)) => {
            typeck_throw(tccx, throw_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Break(break_expr)) => {
            typeck_break(tccx, break_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Continue(continue_expr)) => {
            typeck_continue(tccx, continue_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Fallthrough(fallthrough_expr)) => {
            typeck_fallthrough(tccx, fallthrough_expr).map(|()| tccx.ty(TyKind::Never))
        }
    }?;

    tccx.body.set_node_ty(expr.node_id(), ty.clone());
    Ok(ty)
}

/// Errors inside a block-like are already reported, so it is given the type
/// `never`, which fits anywhere, rather than report more about its value.
fn recover<F>(tccx: &mut TypeckCtxt, typeck: F) -> Ty
where
    F: FnOnce(&mut TypeckCtxt) -> Result<Ty, ()>,
{
    typeck(tccx).unwrap_or_else(|()| tccx.ty(TyKind::Never))
}

fn typeck_forced_try_expr(
    tccx: &mut TypeckCtxt,
    forced_try_expr: &AstForcedTryExpr,
) -> Result<Ty, TypeError> {
    let (success_ty, _) =
        typeck_handled_call(tccx, &forced_try_expr.call_expr, forced_try_expr.node_id)?;
    Ok(success_ty)
}

fn typeck_try_catch_expr(
    tccx: &mut TypeckCtxt,
    try_catch_expr: &AstTryCatchExpr,
) -> Result<Ty, TypeError> {
    let (success_ty, throws_ty) =
        typeck_handled_call(tccx, &try_catch_expr.call_expr, try_catch_expr.node_id)?;

    // The binding gets its own scope around the body's, keyed by this expression.
    tccx.open_scope(try_catch_expr.node_id);
    if let Some(binding) = &try_catch_expr.binding {
        // false because the binding is never mutable.
        tccx.declare_local(binding.node_id, &binding.text, throws_ty, false);
    }
    let body_ty = typeck_block(tccx, &try_catch_expr.body, false, Some(&success_ty));
    tccx.close_scope();

    // Errors inside the body are already reported, the expression still has its type.
    if let Ok(body_ty) = body_ty
        && !body_ty.coerces_to(&success_ty)
    {
        let body_node_id = try_catch_expr
            .body
            .stmts
            .last()
            .map_or(try_catch_expr.body.node_id, |stmt| stmt.inner_node_id());
        return Err(TypeError::IncompatibleTypes(
            body_ty,
            success_ty,
            body_node_id,
            try_catch_expr.call_expr.node_id,
        ));
    }

    Ok(success_ty)
}

/// Typechecks a call whose error is handled by `try!` or `try/catch`,
/// returning its (success, thrown) types.
fn typeck_handled_call(
    tccx: &mut TypeckCtxt,
    call_expr: &ast::AstCallExpr,
    try_node_id: NodeId,
) -> Result<(Ty, Ty), TypeError> {
    let old_switch = tccx.permit_calls_to_throwing_funcs;
    tccx.permit_calls_to_throwing_funcs = true;
    // Get the inner call expr, let typeck resolve that then just copy that out
    let ty = typeck_call_expr(tccx, call_expr);
    tccx.permit_calls_to_throwing_funcs = old_switch;
    let ty = ty?;

    // the call itself carries the full (success, thrown) type, the try only the success.
    let TyKind::FullFallible(success_ty, throws_ty) = ty.kind() else {
        return Err(TypeError::TryOnNonThrowingCall(try_node_id));
    };
    tccx.body.set_node_ty(call_expr.node_id, ty.clone());
    Ok((success_ty.clone(), throws_ty.clone()))
}

// Constraints have some extra special conditions for the expressions allowed, to avoid
// disambiguity.
//
// This also covers correctly parsing expresions (so `ret` resolves its type as the return type of the constraint),
// `old` resolves its type as the old type of the passed value.
fn typeck_constraint_expr(
    tccx: &mut TypeckCtxt,
    expr: &ast::AstExpr,
) -> Result<Option<Ty>, TypeError> {
    let Some(constraint_type) = tccx.inside_constraint.as_ref() else {
        return Ok(None);
    };

    match &expr.kind {
        ast::AstExprKind::Ident(ident) if ident.is_constraint_kw_ret() => {
            if constraint_type.is_pre() {
                return Err(TypeError::RetInPreConstraint(
                    ident.node_id,
                    constraint_type.node_id(),
                ));
            }

            // Function return type instead
            tccx.body.set_node_res(ident.node_id, Res::ConstraintRet);
            // ensure blocks only run on happy paths
            return Ok(Some(tccx.body.return_ty.clone()));
        }

        ast::AstExprKind::Call(call_expr) if call_expr.is_constraint_kw_old() => {
            if constraint_type.is_pre() {
                return Err(TypeError::OldInPreConstraint(
                    expr.node_id(),
                    constraint_type.node_id(),
                ));
            }

            if call_expr.args.len() != 1 {
                return Err(TypeError::MalformedOldInConstraint(
                    call_expr.node_id,
                    constraint_type.node_id(),
                ));
            }

            let arg = &call_expr.args[0];
            let arg_ty = typeck_expr(tccx, arg)?;
            let old_id = tccx.next_old_id();
            tccx.body.old_tys.push(arg_ty.clone());
            tccx.body
                .set_node_res(call_expr.node_id, Res::ConstraintOld(old_id));
            return Ok(Some(arg_ty));
        }

        // Constraints are single conditions, a block of statements has no place there.
        ast::AstExprKind::TryCatch(try_catch_expr) => {
            return Err(TypeError::TryCatchInConstraint(
                try_catch_expr.node_id,
                constraint_type.node_id(),
            ));
        }

        ast::AstExprKind::Block(_)
        | ast::AstExprKind::If(_)
        | ast::AstExprKind::Loop(_)
        | ast::AstExprKind::While(_)
        | ast::AstExprKind::Switch(_)
        | ast::AstExprKind::Return(_)
        | ast::AstExprKind::Throw(_)
        | ast::AstExprKind::Break(_)
        | ast::AstExprKind::Continue(_)
        | ast::AstExprKind::Fallthrough(_) => {
            return Err(TypeError::ControlFlowInConstraint(
                expr.node_id(),
                constraint_type.node_id(),
            ));
        }

        _ => {}
    }

    Ok(None)
}

fn typeck_path_expr(tccx: &mut TypeckCtxt, path_expr: &ast::AstPathExpr) -> Result<Ty, TypeError> {
    let res = tccx
        .tcx
        .resolve_path_expr(path_expr.clone(), tccx.module_id)?
        .ok_or_else(|| TypeError::CannotResolve(path_expr.field.text.clone(), path_expr.node_id))?;

    let ty = tccx.res_ty(&res).ok_or_else(|| {
        TypeError::UnresolvableType(path_expr.field.text.clone(), path_expr.node_id)
    })?;

    tccx.body.set_node_res(path_expr.node_id, res);
    Ok(ty)
}

fn typeck_implicit_path_expr(
    tccx: &mut TypeckCtxt,
    impl_path_expr: &ast::AstImplicitPathExpr,
) -> Result<Ty, TypeError> {
    let expected = tccx
        .inferrable_ty()
        .ok_or_else(|| TypeError::CannotImplyVariant(impl_path_expr.node_id))?;

    let res = tccx
        .tcx
        .resolve_implicit_path_expr(impl_path_expr.clone(), expected.clone(), tccx.module_id)?
        .ok_or_else(|| TypeError::CannotImplyVariant(impl_path_expr.node_id))?;

    let ty = tccx.res_ty(&res).ok_or_else(|| {
        TypeError::UnresolvableType(impl_path_expr.path.text.clone(), impl_path_expr.node_id)
    })?;

    tccx.body.set_node_res(impl_path_expr.node_id, res);
    Ok(ty)
}

fn typeck_call_expr(tccx: &mut TypeckCtxt, call_expr: &ast::AstCallExpr) -> Result<Ty, TypeError> {
    let func_ty = typeck_expr(tccx, &call_expr.callee)?;
    let TyKind::Func(sig) = func_ty.kind.as_ref() else {
        return Err(TypeError::CannotCallNonFunction(call_expr.callee.node_id()));
    };

    let args = &call_expr.args;
    if args.len() != sig.param_tys.len() {
        return Err(TypeError::IncorrectNumberOfArguments(
            call_expr.node_id,
            args.len(),
            sig.param_tys.len(),
        ));
    }

    // Ensure args match
    for (i, (arg, param_ty)) in args.iter().zip(sig.param_tys.iter()).enumerate() {
        let ity = tccx.begin_inferrable_ty(Some(param_ty.clone()));
        let arg_ty = typeck_expr(tccx, arg);
        tccx.end_inferrable_ty(ity);
        let arg_ty = arg_ty?;
        if arg_ty != *param_ty {
            return Err(TypeError::IncorrectArgumentType(
                param_ty.clone(),
                arg_ty,
                i,
                arg.node_id(),
            ));
        }
    }

    if !tccx.permit_calls_to_throwing_funcs
        && let Some(ref throws_ty) = sig.throws_ty
    {
        return Err(TypeError::DangerousUseThrowingFunction(
            call_expr.node_id,
            sig.return_ty.clone(),
            throws_ty.clone(),
        ));
    }

    match sig.throws_ty {
        None => Ok(sig.return_ty.clone()),
        Some(ref throws_ty) => {
            if !throws_ty.is_scalar() {
                return Err(TypeError::NonScalarFallible(
                    call_expr.node_id,
                    sig.return_ty.clone(),
                ));
            }

            Ok(tccx.ty(TyKind::FullFallible(
                sig.return_ty.clone(),
                throws_ty.clone(),
            )))
        }
    }
}

fn typeck_unary_expr(tccx: &mut TypeckCtxt, un_expr: &ast::AstUnaryExpr) -> Result<Ty, TypeError> {
    let expr_ty = typeck_expr(tccx, &un_expr.expr)?;

    if matches!(
        expr_ty.kind(),
        TyKind::FullFallible(..)
            | TyKind::Never
            | TyKind::Void
            | TyKind::Func(..)
            | TyKind::Enum(..)
    ) {
        return Err(TypeError::CannotUnaryOp(
            expr_ty,
            un_expr.operator,
            un_expr.node_id,
        ));
    }

    match un_expr.operator {
        ast::AstUnaryOperator::Negate if !expr_ty.is_negatable() => {
            Err(TypeError::CannotNegate(expr_ty, un_expr.expr.node_id()))
        }
        ast::AstUnaryOperator::Negate | ast::AstUnaryOperator::Not => Ok(expr_ty),
    }
}

fn typeck_binary_expr(
    tccx: &mut TypeckCtxt,
    bin_expr: &ast::AstBinaryExpr,
) -> Result<Ty, TypeError> {
    let left = typeck_value_expr(tccx, &bin_expr.left)?;
    let right = tccx.with_inferrable(&left, |tccx| typeck_value_expr(tccx, &bin_expr.right))?;
    if left != right {
        return Err(TypeError::IncompatibleTypes(
            left,
            right,
            bin_expr.left.node_id(),
            bin_expr.right.node_id(),
        ));
    }

    if *left.kind() == TyKind::Void {
        return Err(TypeError::ExpressionOnVoid(
            bin_expr.operator,
            bin_expr.node_id,
        ));
    }

    if matches!(*left.kind(), TyKind::Func(..)) {
        return Err(TypeError::ExpressionOnFunc(
            left,
            bin_expr.operator,
            bin_expr.node_id,
        ));
    }

    match &bin_expr.operator {
        ast::AstBinaryOperator::Add
        | ast::AstBinaryOperator::Sub
        | ast::AstBinaryOperator::Mul
        | ast::AstBinaryOperator::Div
            if left.is_bool() =>
        {
            Err(TypeError::CannotCompareBooleans(bin_expr.node_id))
        }
        op @ (ast::AstBinaryOperator::LOr | ast::AstBinaryOperator::LAnd)
            if !left.is_bool() || !right.is_bool() =>
        {
            Err(TypeError::LogicalOpOnNonBool(
                if !left.is_bool() { left } else { right },
                *op,
                bin_expr.node_id,
            ))
        }
        ast::AstBinaryOperator::Add
        | ast::AstBinaryOperator::Sub
        | ast::AstBinaryOperator::Mul
        | ast::AstBinaryOperator::Div => Ok(left),
        ast::AstBinaryOperator::Eq
        | ast::AstBinaryOperator::Ne
        | ast::AstBinaryOperator::Lt
        | ast::AstBinaryOperator::Le
        | ast::AstBinaryOperator::Gt
        | ast::AstBinaryOperator::Ge
        | ast::AstBinaryOperator::LOr
        | ast::AstBinaryOperator::LAnd => Ok(tccx.tcx.bool_ty()),
    }
}

/// Whether something never completes normally (returns, throws, breaks, etc.),
/// plus any dead code found after such a point.
struct Diverges(bool, Vec<NodeId>);

impl Diverges {
    pub fn always() -> Self {
        Self(true, vec![])
    }

    pub fn never() -> Self {
        Self(false, vec![])
    }

    pub fn if_always_diverges(self) -> Option<Self> {
        if self.0 { Some(self) } else { None }
    }

    pub fn and(self, other: Self) -> Self {
        Self(
            self.0 && other.0,
            self.1.iter().chain(other.1.iter()).copied().collect(),
        )
    }
}

/// Does this statement diverge on every path through it? `loops_with_breaks`
/// comes from typeck, as a `break` may be hidden anywhere in a loop's body.
fn diverges(stmt: &ast::AstStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    match &stmt.kind {
        AstStmtKind::Expr(expr)
        | AstStmtKind::BlockLike(expr)
        | AstStmtKind::ImplicitReturn(expr) => expr_diverges(expr, loops_with_breaks),

        // These can't contain a diverging expression without it being used
        // as a value, which typeck rejects.
        AstStmtKind::Print(_)
        | AstStmtKind::Decl(_)
        | AstStmtKind::Assign(_)
        | AstStmtKind::Guard(_)
        | AstStmtKind::Require(_)
        | AstStmtKind::Use(_) => Diverges::never(),
    }
}

fn expr_diverges(expr: &ast::AstExpr, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    match &expr.kind {
        AstExprKind::Return(_)
        | AstExprKind::Throw(_)
        | AstExprKind::Break(_)
        | AstExprKind::Continue(_)
        | AstExprKind::Fallthrough(_) => Diverges::always(),
        AstExprKind::Block(b) => block_diverges(b, loops_with_breaks),
        AstExprKind::If(s) => if_diverges(s, loops_with_breaks),
        AstExprKind::Loop(l) => loop_diverges(l, loops_with_breaks),
        AstExprKind::Switch(s) => switch_diverges(s, loops_with_breaks),

        // Compiler isn't yet smart enough to check this.
        AstExprKind::While(_)
        | AstExprKind::Unary(_)
        | AstExprKind::Binary(_)
        | AstExprKind::Literal(_)
        | AstExprKind::Ident(_)
        | AstExprKind::Call(_)
        | AstExprKind::Path(_)
        | AstExprKind::ImplicitPath(_)
        | AstExprKind::ForcedTry(_)
        | AstExprKind::TryCatch(_) => Diverges::never(),
    }
}

fn switch_diverges(switch_: &ast::AstSwitchStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    let mut diverges = Diverges::always();
    for case in switch_.cases.iter() {
        let (AstSwitchCase::Case(AstSwitchCaseItem { body, .. })
        | AstSwitchCase::Else(AstSwitchElseCase { body, .. })) = case;

        // typeck requires switches to be exhaustive, so if all blocks diverge
        // then it will. A `fallthrough` counts, as the case it enters must too.
        diverges = diverges.and(block_diverges(body, loops_with_breaks));
    }

    diverges
}

fn loop_diverges(loop_: &ast::AstLoopStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    // Only a `break` ends a `loop`, whatever its body does.
    let Diverges(_, dead_code) = block_diverges(&loop_.body, loops_with_breaks);
    Diverges(!loops_with_breaks.contains(&loop_.node_id), dead_code)
}

fn block_diverges(block: &ast::AstBlockStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    let position = block.stmts.iter().enumerate().find_map(|(i, stmt)| {
        diverges(stmt, loops_with_breaks)
            .if_always_diverges()
            .map(|r| (i, r))
    });

    if let Some((i, mut r)) = position {
        if i + 1 != block.stmts.len() {
            r.1.push(block.stmts[i + 1].inner_node_id());
        }
        r
    } else {
        Diverges::never()
    }
}

fn if_diverges(s: &ast::AstIfStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    // If no else it may continue, so we can't say for sure.
    let Some(else_) = &s.else_ else {
        return Diverges::never();
    };

    block_diverges(&s.then, loops_with_breaks).and(match else_ {
        ast::AstElseBranch::Block(b) => block_diverges(b, loops_with_breaks),
        ast::AstElseBranch::If(nested) => if_diverges(nested, loops_with_breaks),
    })
}
