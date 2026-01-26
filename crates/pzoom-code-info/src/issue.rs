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
    InvalidMethodCall,
    InvalidStaticMethodCall,
    InvalidPropertyFetch,
    InvalidStaticPropertyFetch,
    InvalidCast,
    InvalidClone,

    // Possibly invalid (nullable/optional concerns)
    PossiblyInvalidArgument,
    PossiblyInvalidMethodCall,
    PossiblyInvalidPropertyFetch,
    PossiblyInvalidArrayAccess,
    PossiblyInvalidArrayOffset,
    PossiblyNullArgument,
    PossiblyNullReference,
    PossiblyNullPropertyFetch,
    PossiblyNullArrayAccess,
    PossiblyNullFunctionCall,
    PossiblyUndefinedVariable,
    PossiblyUndefinedArrayOffset,

    // Array access issues
    NullArrayAccess,
    InvalidArrayOffset,

    // Null/Void reference
    NullReference,
    PossiblyUndefinedGlobalVariable,

    // Undefined entities
    UndefinedClass,
    UndefinedInterface,
    UndefinedTrait,
    UndefinedFunction,
    UndefinedMethod,
    UndefinedProperty,
    UndefinedConstant,
    UndefinedVariable,
    UndefinedGlobalVariable,
    UndefinedThisPropertyFetch,
    UndefinedThisPropertyAssignment,

    // Missing items
    MissingReturnType,
    MissingParamType,
    MissingPropertyType,
    MissingConstructor,
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
    RedundantCast,
    RedundantPropertyInitialization,

    // Type system issues
    MixedAssignment,
    MixedArgument,
    MixedReturnStatement,
    MixedPropertyFetch,
    MixedMethodCall,
    MixedArrayAccess,
    MixedArrayOffset,
    MixedArrayAssignment,

    // Less specific types
    LessSpecificReturnType,
    MoreSpecificReturnType,
    LessSpecificImplementedReturnType,

    // Docblock issues
    InvalidDocblock,
    InvalidDocblockParamName,
    MismatchingDocblockParamType,
    MismatchingDocblockReturnType,

    // Deprecated code
    DeprecatedClass,
    DeprecatedInterface,
    DeprecatedMethod,
    DeprecatedProperty,
    DeprecatedFunction,
    DeprecatedConstant,

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

    // Argument coercion
    ArgumentTypeCoercion,

    // Named argument issues
    NamedArgumentNotAllowed,
    InvalidNamedArgument,
    PositionalArgAfterNamed,

    // Missing docblock type
    MissingDocblockType,

    // Invalid operand for operations
    InvalidOperand,
    InvalidArrayOperand,

    // Class issues
    AbstractInstantiation,
    UnimplementedAbstractMethod,
    UnimplementedInterfaceMethod,
    DuplicateClass,
    DuplicateMethod,
    DuplicateFunction,
    CircularReference,
    InvalidExtends,
    InvalidImplements,

    // Method signature issues
    MethodSignatureMismatch,
    ParamNameMismatch,
    ConstructorSignatureMismatch,
    MoreSpecificImplementedParamType,
    LessSpecificImplementedParamType,

    // Property issues
    PropertyNotSetInConstructor,
    UninitializedProperty,
    ReadonlyPropertyAssignment,
    InvalidPropertyAssignment,
    UndefinedPropertyAssignment,
    UndefinedPropertyFetch,
    NullPropertyAssignment,
    NullPropertyFetch,
    PossiblyNullPropertyAssignment,
    PossiblyInvalidPropertyAssignmentValue,
    PropertyTypeCoercion,
    MixedPropertyTypeCoercion,

    // Reserved word issues
    ReservedWord,

    // Pass by reference issues
    InvalidPassByReference,

    // Scalar type issues
    InvalidScalarArgument,
    InvalidStringClass,

    // Control flow issues
    ContinueOutsideLoop,
    BreakOutsideLoop,
    NullArgument,

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
            | IssueKind::UndefinedInterface
            | IssueKind::UndefinedTrait
            | IssueKind::UndefinedFunction
            | IssueKind::UndefinedMethod
            | IssueKind::UndefinedProperty
            | IssueKind::UndefinedConstant
            | IssueKind::UndefinedVariable => IssueSeverity::Error,

            IssueKind::InvalidArgument
            | IssueKind::InvalidReturnType
            | IssueKind::InvalidReturnStatement
            | IssueKind::InvalidPropertyAssignmentValue => IssueSeverity::Error,

            IssueKind::DeprecatedClass
            | IssueKind::DeprecatedInterface
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
