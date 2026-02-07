//! Issue types for static analysis.
//!
//! Modeled after Psalm's extensive issue catalog.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

/// An issue detected during analysis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub message: String,
    pub file_path: StrId,
    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub start_column: u32,
}

/// Categories of issues that can be detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueKind {
    // Type mismatches
    InvalidArgument,
    InvalidReturnType,
    InvalidReturnStatement,
    InvalidPropertyAssignmentValue,
    InvalidArrayAccess,
    InvalidFunctionCall,
    InvalidMethodCall,
    InvalidStaticMethodCall,
    InvalidPropertyFetch,
    InvalidStaticPropertyFetch,
    InvalidClass,
    InvalidParamDefault,
    InvalidScope,
    InvalidGlobal,
    InvalidCast,
    InvalidClone,
    MixedClone,
    InvalidCatch,
    InvalidThrow,

    // Possibly invalid (nullable/optional concerns)
    PossiblyInvalidArgument,
    PossiblyInvalidMethodCall,
    PossiblyInvalidPropertyFetch,
    PossiblyInvalidArrayAccess,
    PossiblyInvalidArrayOffset,
    PossiblyInvalidClone,
    PossiblyNullArgument,
    PossiblyNullReference,
    PossiblyFalseReference,
    PossiblyNullPropertyFetch,
    PossiblyNullArrayAccess,
    PossiblyNullFunctionCall,
    PossiblyUndefinedArrayOffset,

    // Array access issues
    NullArrayAccess,
    InvalidArrayOffset,
    DuplicateArrayKey,
    InvalidArrayAssignment,

    // Null/Void reference
    NullReference,

    // Undefined entities
    UndefinedClass,
    UndefinedAttributeClass,
    UndefinedDocblockClass,
    UndefinedInterface,
    UndefinedTrait,
    UndefinedFunction,
    UndefinedMethod,
    PossiblyUndefinedMethod,
    UndefinedMagicMethod,
    UndefinedInterfaceMethod,
    UndefinedProperty,
    UndefinedConstant,
    UndefinedVariable,
    UndefinedGlobalVariable,
    UndefinedThisPropertyFetch,
    UndefinedThisPropertyAssignment,
    UndefinedMagicPropertyFetch,
    UndefinedMagicPropertyAssignment,

    // Missing items
    MissingReturnType,
    MissingParamType,
    MissingPropertyType,
    MissingConstructor,
    MissingDependency,
    MissingClosureReturnType,
    MissingClosureParamType,

    // Access violations
    InaccessibleMethod,
    InaccessibleProperty,
    InaccessibleClassConstant,

    // Unused code
    UnusedVariable,
    UnusedParameter,
    UnusedProperty,
    UnusedMethod,
    UnusedClass,
    UnusedFunction,
    UnusedClosureParam,

    // Redundant code
    RedundantCondition,
    RedundantConditionGivenDocblockType,
    ParadoxicalCondition,
    UnevaluatedCode,
    RedundantFunctionCall,
    RedundantCast,
    RedundantPropertyInitializationCheck,
    RedundantPropertyInitialization,

    // Type system issues
    MixedAssignment,
    MixedArgument,
    MixedReturnStatement,
    MixedReturnTypeCoercion,
    MixedPropertyFetch,
    MixedMethodCall,
    MixedArrayAccess,
    MixedArrayOffset,
    MixedArrayAssignment,
    MixedStringOffsetAssignment,

    // Less specific types
    LessSpecificReturnType,
    MoreSpecificReturnType,
    LessSpecificImplementedReturnType,
    ImplementedReturnTypeMismatch,
    ImplementedParamTypeMismatch,

    // Docblock issues
    InvalidDocblock,
    PossiblyInvalidDocblockTag,
    InvalidDocblockParamName,
    MismatchingDocblockParamType,
    MismatchingDocblockReturnType,

    // Deprecated code
    DeprecatedClass,
    DeprecatedInterface,
    DeprecatedTrait,
    DeprecatedMethod,
    DeprecatedProperty,
    DeprecatedFunction,
    DeprecatedConstant,

    // Forbidden code
    ForbiddenCode,

    // Internal code
    InternalClass,
    InternalMethod,
    InternalProperty,

    // Taint/security issues
    TaintedInput,
    TaintedSql,
    TaintedHtml,
    TaintedShell,
    TaintedFile,
    TaintedHeader,
    TaintedInclude,
    TaintedEval,
    TaintedUnserialize,

    // Comparison issues
    TypeDoesNotContainType,
    TypeDoesNotContainNull,
    DocblockTypeContradiction,
    RiskyTruthyFalsyComparison,

    // Loop issues
    LoopInvalidation,
    PossiblyInvalidIterator,

    // Return issues
    InvalidReturnNull,
    InvalidNullableReturnType,
    InvalidVoid,
    InvalidFalsableReturnType,
    NullableReturnStatement,
    FalsableReturnStatement,
    LessSpecificReturnStatement,

    // General
    ParseError,
    InternalError,
    UnrecognizedExpression,

    // Argument coercion
    ArgumentTypeCoercion,
    MixedArgumentTypeCoercion,

    // Named argument issues
    NamedArgumentNotAllowed,
    InvalidNamedArgument,
    PositionalArgAfterNamed,

    // Missing docblock type
    MissingDocblockType,

    // Invalid operand for operations
    InvalidOperand,
    NullOperand,
    FalseOperand,
    PossiblyNullOperand,
    PossiblyInvalidOperand,
    InvalidArrayOperand,

    // Class issues
    AbstractInstantiation,
    InterfaceInstantiation,
    UnimplementedAbstractMethod,
    UnimplementedInterfaceMethod,
    DuplicateClass,
    DuplicateMethod,
    DuplicateFunction,
    DuplicateParam,
    MissingTemplateParam,
    CircularReference,
    InvalidExtendClass,
    InvalidImplements,
    InvalidInterfaceImplementation,
    InvalidAttribute,
    InvalidTraversableImplementation,
    InvalidEnumBackingType,
    InvalidEnumCaseValue,
    InvalidEnumMethod,
    NoEnumProperties,
    DuplicateEnumCase,
    DuplicateEnumCaseValue,
    UnhandledMatchCondition,

    // Method signature issues
    MethodSignatureMismatch,
    TraitMethodSignatureMismatch,
    MethodSignatureMustOmitReturnType,
    ParamNameMismatch,
    ConstructorSignatureMismatch,
    OverriddenMethodAccess,
    MoreSpecificImplementedParamType,
    LessSpecificImplementedParamType,

    // Property issues
    DuplicateProperty,
    PropertyNotSetInConstructor,
    UninitializedProperty,
    ReadonlyPropertyAssignment,
    InvalidPropertyAssignment,
    UndefinedPropertyAssignment,
    UndefinedPropertyFetch,
    NoInterfaceProperties,
    NullPropertyAssignment,
    NullPropertyFetch,
    PossiblyNullPropertyAssignment,
    PossiblyInvalidPropertyAssignmentValue,
    PropertyTypeCoercion,
    MixedPropertyTypeCoercion,
    OverriddenPropertyAccess,
    NonInvariantPropertyType,
    NonInvariantDocblockPropertyType,

    // Reserved word issues
    ReservedWord,

    // Call style issues
    DirectConstructorCall,
    AbstractMethodCall,
    ParentNotFound,

    // Pass by reference issues
    InvalidPassByReference,
    ReferenceConstraintViolation,
    ConflictingReferenceConstraint,
    NonVariableReferenceReturn,
    ReferenceReusedFromConfusingScope,
    UnsupportedReferenceUsage,

    // Scalar type issues
    InvalidScalarArgument,
    InvalidStringClass,
    InvalidStaticInvocation,
    NonStaticSelfCall,
    StringIncrement,
    ImplicitToStringCast,
    InvalidToString,

    // Control flow issues
    ContinueOutsideLoop,
    BreakOutsideLoop,
    NullArgument,
    ImpureFunctionCall,
    ImpureMethodCall,
    ImpurePropertyAssignment,
    MissingImmutableAnnotation,
    MutableDependency,
    IfThisIsMismatch,
    Trace,

    // Argument count issues
    TooFewArguments,
    TooManyArguments,
}

impl Issue {
    pub fn new(
        kind: IssueKind,
        message: impl Into<String>,
        file_path: StrId,
        start_offset: u32,
        end_offset: u32,
        start_line: u32,
        start_column: u32,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            file_path,
            start_offset,
            end_offset,
            start_line,
            start_column,
        }
    }

    /// Get the severity level of this issue.
    pub fn severity(&self) -> IssueSeverity {
        match self.kind {
            IssueKind::ParseError | IssueKind::InternalError => IssueSeverity::Error,

            IssueKind::TaintedInput
            | IssueKind::TaintedSql
            | IssueKind::TaintedHtml
            | IssueKind::TaintedShell
            | IssueKind::TaintedFile
            | IssueKind::TaintedHeader
            | IssueKind::TaintedInclude
            | IssueKind::TaintedEval
            | IssueKind::TaintedUnserialize => IssueSeverity::Error,

            IssueKind::UndefinedClass
            | IssueKind::UndefinedAttributeClass
            | IssueKind::UndefinedDocblockClass
            | IssueKind::UndefinedInterface
            | IssueKind::UndefinedTrait
            | IssueKind::UndefinedFunction
            | IssueKind::UndefinedMethod
            | IssueKind::PossiblyUndefinedMethod
            | IssueKind::UndefinedMagicMethod
            | IssueKind::UndefinedInterfaceMethod
            | IssueKind::UndefinedProperty
            | IssueKind::UndefinedMagicPropertyFetch
            | IssueKind::UndefinedMagicPropertyAssignment
            | IssueKind::UndefinedConstant
            | IssueKind::UndefinedVariable
            | IssueKind::InvalidClass
            | IssueKind::ForbiddenCode => IssueSeverity::Error,

            IssueKind::InvalidArgument
            | IssueKind::InvalidReturnType
            | IssueKind::InvalidReturnStatement
            | IssueKind::InvalidPropertyAssignmentValue
            | IssueKind::InvalidScope
            | IssueKind::InvalidGlobal => IssueSeverity::Error,

            IssueKind::DeprecatedClass
            | IssueKind::DeprecatedInterface
            | IssueKind::DeprecatedTrait
            | IssueKind::DeprecatedMethod
            | IssueKind::DeprecatedProperty
            | IssueKind::DeprecatedFunction
            | IssueKind::DeprecatedConstant => IssueSeverity::Warning,

            IssueKind::UnusedVariable
            | IssueKind::UnusedParameter
            | IssueKind::UnusedProperty
            | IssueKind::UnusedMethod
            | IssueKind::UnusedClass
            | IssueKind::UnusedFunction
            | IssueKind::UnusedClosureParam => IssueSeverity::Info,

            _ => IssueSeverity::Error,
        }
    }
}

/// Severity level for issues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
}
