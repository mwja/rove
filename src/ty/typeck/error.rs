//! Type errors and their diagnostics.

use super::*;

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
    #[error("value of type {0} has no field {1}")]
    TypeHasNoField(Ty, String, NodeId),
    #[error("operator is not implemented on this type")]
    OperatorNotImplementedOnType(Ty, ast::AstBinaryOperator, NodeId),
    #[error("struct field {0} not found")]
    StructFieldNotFound(String, NodeId),
    #[error("attempted to use non-struct as a struct constructor")]
    AttemptedToUseNonStructAsConstructor(NodeId),
    #[error("struct field {0} is initialised more than once")]
    DuplicateStructInitField(String, NodeId, NodeId),
    #[error("missing struct fields: {}", .0.join(", "))]
    MissingStructInitFields(Vec<String>, NodeId),
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
            TypeHasNoField(..) => Some(1052),
            OperatorNotImplementedOnType(..) => Some(1053),
            StructFieldNotFound(..) => Some(1054),
            AttemptedToUseNonStructAsConstructor(..) => Some(1055),
            DuplicateStructInitField(..) => Some(1056),
            MissingStructInitFields(..) => Some(1057),

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
            TypeHasNoField(ty, field_name, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to access field `{field_name}` of type {ty} here"),
                        ),
                ]
            }
            OperatorNotImplementedOnType(ty, op, node) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node,
                            format!("attempted to perform operator {} on type {ty} here", op),
                        )
                        .with_help(Some("this operator is not implemented for this type (and may not be implemented yet)")),
                ]
            }
            StructFieldNotFound(field_name, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to access field `{field_name}` of a struct here"),
                        )
                        .with_help(Some("this field does not exist on this struct")),
                ]
            }
            AttemptedToUseNonStructAsConstructor(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "attempted to use a non-struct as a struct constructor here",
                        )
                        .with_help(Some("did you mean to use a struct type here?")),
                ]
            }
            DuplicateStructInitField(field_name, first, duplicate) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            first,
                            format!("`{field_name}` is first initialised here"),
                        )
                        .with_label_from(recorder, duplicate, "and initialised again here"),
                ]
            }
            MissingStructInitFields(fields, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!(
                                "this struct is missing {}",
                                fields
                                    .iter()
                                    .map(|f| format!("`{f}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )
                        .with_help(Some("every field must be given a value")),
                ]
            }
        }
    }
}
