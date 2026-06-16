//! Issue types for static analysis.
//!
//! Modeled after Psalm's extensive issue catalog.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::code_location::CodeLocation;

/// A supporting location for an issue: somewhere else in the source that
/// explains or contributes to the problem (e.g. the declaration a return
/// statement violates, or the origin of a mixed value), with its own message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecondaryLocation {
    pub location: CodeLocation,
    pub message: String,
}

impl SecondaryLocation {
    pub fn new(location: CodeLocation, message: impl Into<String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

/// An issue detected during analysis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub message: String,
    /// Where in the source the issue points (file + byte/line/column span).
    pub location: CodeLocation,
    /// Supporting locations displayed under the primary message, each with
    /// its own explanatory message.
    #[serde(default)]
    pub secondary_locations: Vec<SecondaryLocation>,
    /// For Tainted* issues: the source-to-sink node chain (Psalm's
    /// `IssueData::$taint_trace`), rendered as labelled snippets by the
    /// console reporter. The message still embeds the Hakana-style
    /// `in path: ...` summary.
    #[serde(default)]
    pub taint_trace: Vec<TraceNode>,
    /// Psalm's `CodeIssue::$dupe_key`: when set, same-kind issues at the same
    /// line and column with an equal dupe key collapse to one, even when the
    /// human-readable messages differ (e.g. the reconciler's "Docblock-defined
    /// type int for $x is never null" vs the assertion finder's "int does not
    /// contain null" both carry the key "int null").
    #[serde(default)]
    pub dupe_key: Option<String>,
}

/// One step of a taint trace: a label plus the position it occurred at, if
/// known (`$_GET` and other synthetic sources have none).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceNode {
    pub label: String,
    pub location: Option<CodeLocation>,
}

/// Categories of issues that can be detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueKind {
    // Type mismatches
    InvalidArgument,
    /// An argument whose type is `never`/`nothing` — its possible types were all
    /// invalidated, signalling dead/unreachable code. Mirrors Psalm/Hakana `NoValue`.
    NoValue,
    InvalidReturnType,
    InvalidReturnStatement,
    InvalidPropertyAssignmentValue,
    InvalidArrayAccess,
    /// Fetch from a provably-empty array (Psalm shortcode 100, level -1).
    EmptyArrayAccess,
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
    PossiblyInvalidFunctionCall,
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
    /// A literal int offset into an array whose presence is not proven, under
    /// `ensureArrayIntOffsetsExist`.
    PossiblyUndefinedIntArrayOffset,
    /// A literal string offset into an array whose presence is not proven, under
    /// `ensureArrayStringOffsetsExist`.
    PossiblyUndefinedStringArrayOffset,
    /// A variable assigned on some but not all paths to its use.
    PossiblyUndefinedVariable,
    /// A global-scope variable assigned on some but not all paths to its use.
    PossiblyUndefinedGlobalVariable,

    // Array access issues
    NullArrayAccess,
    InvalidArrayOffset,
    DuplicateArrayKey,
    InvalidArrayAssignment,
    PossiblyInvalidArrayAssignment,

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
    /// A class constant declared without a native type on PHP >= 8.3 in a
    /// non-final class (Psalm shortcode 359).
    MissingClassConstType,
    MissingConstructor,
    MissingDependency,
    ExtensionRequirementViolation,
    ImplementationRequirementViolation,
    MissingClosureReturnType,
    MissingClosureParamType,

    // Access violations
    InaccessibleMethod,
    InaccessibleProperty,
    InaccessibleClassConstant,

    // Unused code
    UnusedVariable,
    UnusedParam,
    UnusedProperty,
    UnusedMethod,
    UnusedClass,
    UnusedFunction,
    UnusedClosureParam,
    /// A `@psalm-suppress` annotation that never suppressed anything
    /// (Psalm's findUnusedPsalmSuppress feature).
    UnusedPsalmSuppress,
    /// A foreach value variable that is never referenced in the loop body.
    UnusedForeachValue,
    /// An inline `@var` annotation matching the inferred type exactly.
    UnnecessaryVarAnnotation,
    /// A pure function's return value discarded (Psalm find_unused_code).
    UnusedFunctionCall,
    /// A mutation-free method's return value discarded (Psalm find_unused_code).
    UnusedMethodCall,
    /// A public/protected method never referenced inside the codebase.
    PossiblyUnusedMethod,
    /// A public/protected property never referenced inside the codebase.
    PossiblyUnusedProperty,
    /// A parameter never referenced in any call site.
    PossiblyUnusedParam,
    /// A non-void method whose return value is never used at any call site.
    PossiblyUnusedReturnValue,
    /// A non-void *private* method whose return value is never used (Psalm
    /// reports the definite UnusedReturnValue for private methods).
    UnusedReturnValue,
    /// A private constructor that is never called (Psalm find_unused_code).
    UnusedConstructor,
    /// A docblock `@param` for a parameter that does not exist.
    UnusedDocblockParam,
    /// A class with no descendants that Psalm requires to be final
    /// (find_unused_code's ClassMustBeFinal).
    ClassMustBeFinal,

    // Redundant code
    RedundantCondition,
    RedundantConditionGivenDocblockType,
    ParadoxicalCondition,
    UnevaluatedCode,
    RedundantFunctionCall,
    RedundantFunctionCallGivenDocblockType,
    RedundantCast,
    RedundantFlag,
    RedundantPropertyInitializationCheck,
    RedundantPropertyInitialization,

    // Method/function complexity (Psalm's limitMethodComplexity)
    ComplexMethod,
    ComplexFunction,

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
    MismatchingDocblockPropertyType,
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
    TaintedCallable,
    TaintedCookie,
    TaintedExtract,
    TaintedLdap,
    TaintedSleep,
    TaintedSSRF,
    TaintedXpath,
    TaintedTextWithQuotes,
    TaintedUserSecret,
    TaintedSystemSecret,

    // Comparison issues
    TypeDoesNotContainType,
    TypeDoesNotContainNull,
    DocblockTypeContradiction,
    RiskyTruthyFalsyComparison,

    // Loop issues
    LoopInvalidation,
    InvalidIterator,
    PossiblyInvalidIterator,
    /// Iterating over a concrete object that does not implement `Traversable`
    /// (PHP iterates its public properties). Mirrors Psalm's `RawObjectIteration`.
    RawObjectIteration,
    /// Like `RawObjectIteration` but the value is only possibly an object.
    PossibleRawObjectIteration,

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
    UnrecognizedStatement,

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
    UnsafeInstantiation,
    UnsafeGenericInstantiation,
    UnimplementedAbstractMethod,
    UnimplementedInterfaceMethod,
    DuplicateClass,
    DuplicateMethod,
    DuplicateFunction,
    DuplicateParam,
    AssignmentToVoid,
    InvalidParent,
    MissingTemplateParam,
    CircularReference,
    InvalidExtendClass,
    InvalidImplements,
    InvalidInterfaceImplementation,
    InvalidAttribute,
    InheritorViolation,
    PrivateFinalMethod,
    ConstantDeclarationInTrait,
    InvalidTypeImport,
    InvalidTraversableImplementation,
    InvalidEnumBackingType,
    InvalidEnumCaseValue,
    InvalidEnumMethod,
    NoEnumProperties,
    DuplicateEnumCase,
    DuplicateEnumCaseValue,
    UnhandledMatchCondition,

    // Method signature issues
    InvalidOverride,
    MissingOverrideAttribute,
    MethodSignatureMismatch,
    TraitMethodSignatureMismatch,
    MethodSignatureMustOmitReturnType,
    MethodSignatureMustProvideReturnType,
    ParamNameMismatch,
    ConstructorSignatureMismatch,
    OverriddenMethodAccess,
    MoreSpecificImplementedParamType,
    LessSpecificImplementedParamType,

    // Include issues
    UnresolvableInclude,
    /// A `require`/`include` of a resolvable path that does not exist on disk.
    MissingFile,

    // Constant issues
    AmbiguousConstantInheritance,
    InvalidConstantAssignmentValue,
    UnresolvableConstant,
    InvalidClassConstantType,
    LessSpecificClassConstantType,
    OverriddenFinalConstant,
    OverriddenInterfaceConstant,

    // Property issues
    DuplicateConstant,
    DuplicateProperty,
    PropertyNotSetInConstructor,
    UninitializedProperty,
    ReadonlyPropertyAssignment,
    InvalidPropertyAssignment,
    PossiblyInvalidPropertyAssignment,
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
    /// A reference taken to an object/static property (`$b = &$a->b;`) —
    /// Psalm cannot track the property through the reference (shortcode 321).
    UnsupportedPropertyReferenceUsage,

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
    ImpurePropertyFetch,
    ImpureStaticProperty,
    ImpureStaticVariable,
    ImpureVariable,
    ImpureByReferenceAssignment,
    MissingImmutableAnnotation,
    MutableDependency,
    IfThisIsMismatch,
    Trace,
    UndefinedTrace,
    CheckType,

    // Argument count issues
    TooFewArguments,
    TooManyArguments,

    // Additional Psalm issue kinds (added for parity with Psalm's catalog).
    /// A possibly-`false` value passed to a parameter that does not accept it.
    PossiblyFalseArgument,
    PossiblyFalseOperand,
    PossiblyFalseIterator,
    PossiblyFalsePropertyAssignmentValue,
    /// A literal value passed where a non-literal is expected (`expect_variable`).
    InvalidLiteralArgument,
    /// A docblock template parameter that does not satisfy its constraint.
    InvalidTemplateParam,
    /// Type-variable bounds that cannot be reconciled (Hakana's
    /// `IncompatibleTypeParameters`, raised at the end of function analysis).
    IncompatibleTypeParameters,
    TooManyTemplateParams,
    MixedOperand,
    MixedFunctionCall,
    MixedArrayTypeCoercion,
    MixedPropertyAssignment,
    NullIterator,
    NullFunctionCall,
    NullArrayOffset,
    PossiblyNullIterator,
    PossiblyNullArrayOffset,
    PossiblyNullArrayAssignment,
    PossiblyNullPropertyAssignmentValue,
    PossiblyInvalidCast,
    RiskyCast,
    RedundantCastGivenDocblockType,
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
            location: CodeLocation::new(
                file_path,
                start_offset,
                end_offset,
                start_line,
                start_column,
            ),
            secondary_locations: Vec::new(),
            taint_trace: Vec::new(),
            dupe_key: None,
        }
    }

    /// Psalm's dupe-key (builder-style): same-kind issues at the same
    /// position with an equal key collapse to one.
    pub fn with_dupe_key(mut self, dupe_key: impl Into<String>) -> Self {
        self.dupe_key = Some(dupe_key.into());
        self
    }

    /// Construct an issue from a pre-built [`CodeLocation`].
    pub fn at(kind: IssueKind, message: impl Into<String>, location: CodeLocation) -> Self {
        Self {
            kind,
            message: message.into(),
            location,
            secondary_locations: Vec::new(),
            taint_trace: Vec::new(),
            dupe_key: None,
        }
    }

    /// Attach a supporting location (builder-style).
    pub fn with_secondary(mut self, location: CodeLocation, message: impl Into<String>) -> Self {
        self.secondary_locations
            .push(SecondaryLocation::new(location, message));
        self
    }

    /// Attach an optional supporting location (builder-style); `None` is a
    /// no-op so call sites can thread `Option`s without branching.
    pub fn with_secondary_opt(mut self, secondary: Option<SecondaryLocation>) -> Self {
        if let Some(secondary) = secondary {
            self.secondary_locations.push(secondary);
        }
        self
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
            | IssueKind::UnusedParam
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
