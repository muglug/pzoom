//! Cast expression analyzer.

use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};

use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze a cast expression.
///
/// This handles type casts like (int), (string), (array), etc.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the inner expression
    let inner_pos = expression_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let inner_type = analysis_data.expr_types.get(&inner_pos).cloned();

    // Check for redundant casts
    if let Some(ref inner) = inner_type {
        if is_redundant_cast(&unary.operator, &inner) {
            let type_key = inner.get_id(Some(analyzer.interner));
            let (line, col) = analyzer.get_line_column(pos.0);
            // Psalm's handleRedundantCast: a redundancy that follows from a
            // docblock-provided type is the distinct RedundantCastGivenDocblockType
            // (which a project may want, e.g. when guarding untrusted input).
            let (kind, message) = if inner.from_docblock {
                (
                    IssueKind::RedundantCastGivenDocblockType,
                    format!("Redundant cast to {type_key} given docblock-provided type"),
                )
            } else {
                (
                    IssueKind::RedundantCast,
                    format!("Redundant cast to {type_key}"),
                )
            };
            analysis_data.add_issue(Issue::new(
                kind,
                message,
                analyzer.file_path,
                pos.0, // start_offset
                pos.1, // end_offset
                line,
                col,
            ));
        }
    }

    let inner_union = inner_type
        .map(|inner| (*inner).clone())
        .unwrap_or_else(TUnion::mixed);

    // Psalm CastAnalyzer int/float classification: arrays are a RiskyCast
    // (collapse to 0/1), objects without a pseudo-castable ancestor are an
    // InvalidCast — and invalid takes precedence over risky.
    if matches!(
        &unary.operator,
        UnaryPrefixOperator::IntCast(_, _)
            | UnaryPrefixOperator::IntegerCast(_, _)
            | UnaryPrefixOperator::FloatCast(_, _)
            | UnaryPrefixOperator::DoubleCast(_, _)
            | UnaryPrefixOperator::RealCast(_, _)
    ) {
        let to_float = matches!(
            &unary.operator,
            UnaryPrefixOperator::FloatCast(_, _)
                | UnaryPrefixOperator::DoubleCast(_, _)
                | UnaryPrefixOperator::RealCast(_, _)
        );
        let target = if to_float { "float" } else { "int" };

        let mut invalid_cast: Option<String> = None;
        let mut risky_cast: Option<String> = None;
        for atomic in &inner_union.types {
            match atomic {
                TAtomic::TArray { .. } => {
                    if risky_cast.is_none() {
                        risky_cast = Some(atomic.get_id(Some(analyzer.interner)));
                    }
                }
                TAtomic::TNamedObject { name, .. } => {
                    if !named_object_is_pseudo_castable(analyzer, *name) && invalid_cast.is_none() {
                        invalid_cast = Some(atomic.get_id(Some(analyzer.interner)));
                    }
                }
                TAtomic::TObject | TAtomic::TObjectWithProperties { .. } => {
                    if invalid_cast.is_none() {
                        invalid_cast = Some(atomic.get_id(Some(analyzer.interner)));
                    }
                }
                _ => {}
            }
        }

        let (line, col) = analyzer.get_line_column(pos.0);
        if let Some(invalid_id) = invalid_cast {
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidCast,
                format!("{} cannot be cast to {}", invalid_id, target),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        } else if let Some(risky_id) = risky_cast {
            analysis_data.add_issue(Issue::new(
                IssueKind::RiskyCast,
                if to_float {
                    format!(
                        "Casting {} to float has possibly unintended value of 0.0/1.0",
                        risky_id
                    )
                } else {
                    format!(
                        "Casting {} to int has possibly unintended value of 0/1",
                        risky_id
                    )
                },
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    let result_type = match &unary.operator {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            infer_int_cast_type(&inner_union)
        }

        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),

        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            maybe_emit_invalid_string_cast(analyzer, &inner_union, pos, analysis_data);
            infer_string_cast_type(analyzer, &inner_union)
        }

        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }

        UnaryPrefixOperator::ArrayCast(_, _) => infer_array_cast_type(&inner_union),

        UnaryPrefixOperator::ObjectCast(_, _) => {
            // Psalm's CastAnalyzer: casting a shape gives an
            // object-with-properties (`(object) ["a" => 1]` is `object{a: 1}`);
            // a scalar becomes `object{scalar: T}`. Anything else (or a mixed
            // union member) degrades to plain `object`.
            infer_object_cast_type(&inner_union)
        }

        UnaryPrefixOperator::UnsetCast(_, _) => {
            // (unset) cast always returns null (deprecated in PHP 8)
            TUnion::null()
        }

        UnaryPrefixOperator::VoidCast(_, _) => {
            // (void) cast (for completeness, rarely used)
            TUnion::void()
        }

        // Non-cast operators should not reach here
        _ => TUnion::mixed(),
    };

    // Hakana's `as`-expression handling carries the operand's dataflow onto
    // the converted type (`hint_type.parent_nodes = ternary_type.parent_nodes`);
    // PHP casts are the closest analogue.
    //
    // Psalm's `castIntAttempt`/`castFloatAttempt` and `Cast\Bool_` keep
    // parent nodes only when the *variable-use* graph exists - in the taint
    // graph an int/float/bool cast severs the dataflow (the value can no
    // longer carry tainted text; sleep taints re-enter via int-typed param
    // edges instead). String/array/object casts keep their parents in every
    // graph.
    let drops_taint_parents = matches!(
        &unary.operator,
        UnaryPrefixOperator::IntCast(_, _)
            | UnaryPrefixOperator::IntegerCast(_, _)
            | UnaryPrefixOperator::FloatCast(_, _)
            | UnaryPrefixOperator::DoubleCast(_, _)
            | UnaryPrefixOperator::RealCast(_, _)
            | UnaryPrefixOperator::BoolCast(_, _)
            | UnaryPrefixOperator::BooleanCast(_, _)
    );

    let mut result_type = result_type;
    if !(drops_taint_parents
        && matches!(
            analysis_data.data_flow_graph.kind,
            pzoom_code_info::GraphKind::WholeProgram(_)
        ))
    {
        result_type.parent_nodes = inner_union.parent_nodes.clone();
    }

    // Psalm `castStringAttempt`: an explicit (string) cast of an object routes
    // dataflow through its __toString method.
    if matches!(
        &unary.operator,
        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _)
    ) && inner_union.has_object()
    {
        result_type.parent_nodes.extend(add_to_string_call_dataflow(
            analyzer,
            analysis_data,
            &inner_union,
        ));
    }

    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}

fn infer_int_cast_type(inner_type: &TUnion) -> TUnion {
    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TTrue => casted.push(TAtomic::TLiteralInt { value: 1 }),
            TAtomic::TFalse | TAtomic::TNull => casted.push(TAtomic::TLiteralInt { value: 0 }),
            TAtomic::TBool => {
                casted.push(TAtomic::TLiteralInt { value: 0 });
                casted.push(TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TLiteralInt { value } => casted.push(TAtomic::TLiteralInt { value: *value }),
            TAtomic::TLiteralFloat { value } => casted.push(TAtomic::TLiteralInt {
                value: *value as i64,
            }),
            TAtomic::TIntRange { min, max } => casted.push(TAtomic::TIntRange {
                min: *min,
                max: *max,
            }),
            _ => casted.push(TAtomic::TInt),
        }
    }

    if casted.is_empty() {
        TUnion::int()
    } else {
        TUnion::from_types(casted)
    }
}

fn infer_string_cast_type(analyzer: &StatementsAnalyzer<'_>, inner_type: &TUnion) -> TUnion {
    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TLiteralInt { value } => casted.push(TAtomic::TLiteralString {
                value: value.to_string(),
            }),
            TAtomic::TLiteralFloat { value } => casted.push(TAtomic::TLiteralString {
                value: value.to_string(),
            }),
            // Psalm's CastAnalyzer enumerates bounded int ranges (span < 500)
            // into literal strings; '' and numeric-string then stay distinct
            // through later combinations.
            TAtomic::TIntRange {
                min: Some(min),
                max: Some(max),
            } if max - min < 500 => {
                for value in *min..=*max {
                    casted.push(TAtomic::TLiteralString {
                        value: value.to_string(),
                    });
                }
            }
            TAtomic::TInt | TAtomic::TIntRange { .. } | TAtomic::TFloat | TAtomic::TNumeric => {
                casted.push(TAtomic::TNumericString)
            }
            TAtomic::TNamedObject { .. } => {
                if let Some(to_string_type) = get_to_string_return_type(analyzer, atomic) {
                    if union_is_non_empty_string(&to_string_type) {
                        casted.push(TAtomic::TNonEmptyString);
                    } else {
                        casted.push(TAtomic::TString);
                    }
                } else {
                    casted.push(TAtomic::TString);
                }
            }
            _ => casted.push(TAtomic::TString),
        }
    }

    if casted.is_empty() {
        TUnion::string()
    } else {
        TUnion::from_types(casted)
    }
}

/// Psalm's CastAnalyzer object-cast inference: keyed arrays become
/// object-with-properties, scalars become `object{scalar: T}`, and any other
/// member makes the whole cast a plain `object`.
fn infer_object_cast_type(inner_type: &TUnion) -> TUnion {
    let mut permissible_atomics = Vec::with_capacity(inner_type.types.len());

    for atomic in &inner_type.types {
        match atomic {
            // A shape (keyed array) with known entries — a generic array/list has
            // no known entries and falls through to the plain-object fallback.
            // TODO(unify-array): an empty *sealed* shape `array{}` (old empty
            // TKeyedArray) now also has empty known_values and so falls through to
            // plain `object` instead of `object{}`; the old code only ever saw
            // non-empty shapes here (`[]` was the generic empty `TArray`).
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                permissible_atomics.push(TAtomic::TObjectWithProperties {
                    properties: known_values
                        .iter()
                        .map(|(key, (possibly_undefined, value))| {
                            (key.clone(), (*possibly_undefined, value.clone()))
                        })
                        .collect(),
                    is_stringable: false,
                    is_invokable: false,
                });
            }
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse => {
                let mut properties: rustc_hash::FxHashMap<
                    pzoom_code_info::ArrayKey,
                    (bool, TUnion),
                > = rustc_hash::FxHashMap::default();
                properties.insert(
                    pzoom_code_info::ArrayKey::String("scalar".to_string()),
                    (false, TUnion::new(atomic.clone())),
                );
                permissible_atomics.push(TAtomic::TObjectWithProperties {
                    properties,
                    is_stringable: false,
                    is_invokable: false,
                });
            }
            _ => return TUnion::new(TAtomic::TObject),
        }
    }

    if permissible_atomics.is_empty() {
        TUnion::new(TAtomic::TObject)
    } else {
        TUnion::from_types(permissible_atomics)
    }
}

fn infer_array_cast_type(inner_type: &TUnion) -> TUnion {
    if inner_type.is_mixed() {
        return TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed()));
    }

    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TArray { .. } => casted.push(atomic.clone()),
            // (array) null is the empty array `[]`.
            TAtomic::TNull => casted.push(TAtomic::empty_array()),
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset | TAtomic::TNonEmptyMixed => {
                casted.push(TAtomic::array(TUnion::array_key(), TUnion::mixed()));
            }
            _ => casted.push(TAtomic::non_empty_list(TUnion::new(atomic.clone()))),
        }
    }

    if casted.is_empty() {
        TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed()))
    } else {
        TUnion::from_types(type_combiner::combine(casted, false))
    }
}

/// Taint side of Psalm's `CastAnalyzer::castStringAttempt`: converting an
/// object to a string routes dataflow through its `__toString` method. Each
/// named-object atomic with a `__toString` contributes that method's return
/// node (`CallTo Class::__toString`) as a parent node — with a
/// declaring→appearing edge when `__toString` is inherited, and a TaintSource
/// re-registration when the declaring method is annotated
/// `@psalm-taint-source` (Psalm `MethodCallReturnTypeFetcher::
/// taintMethodCallResult` + `FunctionCallReturnTypeFetcher::taintUsingStorage`).
///
/// Returns the nodes to merge into the casted value's parent nodes; empty
/// outside whole-program (taint) mode.
pub(crate) fn add_to_string_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    value_type: &TUnion,
) -> Vec<pzoom_code_info::DataFlowNode> {
    use pzoom_code_info::FunctionLikeIdentifier;
    use pzoom_code_info::data_flow::node::DataFlowNodeKind;

    if !matches!(
        analysis_data.data_flow_graph.kind,
        pzoom_code_info::GraphKind::WholeProgram(_)
    ) {
        return vec![];
    }

    let mut new_parent_nodes = vec![];

    for atomic in &value_type.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };
        let Some(method_info) = class_info.methods.get(&StrId::TO_STRING) else {
            continue;
        };

        let method_call_node = pzoom_code_info::DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(*name, StrId::TO_STRING),
            None,
            None,
        );

        // Inherited __toString: the declaring class's return node (which the
        // declaring body's return statements feed) flows into the appearing
        // class's node (Psalm's 'parent' path).
        if let Some(declaring_class) = method_info.declaring_class
            && declaring_class != *name
        {
            let declaring_node = pzoom_code_info::DataFlowNode::get_for_method_return(
                &FunctionLikeIdentifier::Method(declaring_class, StrId::TO_STRING),
                None,
                None,
            );
            analysis_data.data_flow_graph.add_path(
                &declaring_node.id,
                &method_call_node.id,
                pzoom_code_info::PathKind::Default,
                vec![],
                vec![],
            );
            analysis_data.data_flow_graph.add_node(declaring_node);
        }

        analysis_data
            .data_flow_graph
            .add_node(method_call_node.clone());

        // `@psalm-taint-source` on __toString (Throwable/Exception stubs):
        // the call node doubles as a taint source.
        let mut source_types = if !method_info.taints.taint_source_types.is_empty() {
            method_info.taints.taint_source_types.clone()
        } else {
            method_info.taints.added_taints.clone()
        };
        source_types.retain(|t| !method_info.taints.removed_taints.contains(t));
        if !source_types.is_empty() {
            analysis_data
                .data_flow_graph
                .add_node(pzoom_code_info::DataFlowNode {
                    id: method_call_node.id.clone(),
                    kind: DataFlowNodeKind::TaintSource {
                        pos: method_call_node.get_pos(),
                        types: source_types,
                    },
                });
        }

        new_parent_nodes.push(method_call_node);
    }

    new_parent_nodes
}

pub(crate) fn maybe_emit_invalid_string_cast(
    analyzer: &StatementsAnalyzer<'_>,
    inner_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    // Psalm CastAnalyzer string classification: scalars/resources/mixed are
    // valid or castable; objects need __toString; arrays and bare objects are
    // invalid. A mixed valid/invalid union reports PossiblyInvalidCast,
    // all-invalid reports InvalidCast.
    let mut has_valid_or_castable = false;
    let mut invalid_cast: Option<String> = None;
    for atomic in &inner_type.types {
        let valid = match atomic {
            TAtomic::TArray { .. } | TAtomic::TObject => false,
            TAtomic::TObjectWithProperties { is_stringable, .. } => *is_stringable,
            TAtomic::TNamedObject { name, .. } => {
                !should_emit_invalid_cast_for_named_object(analyzer, *name)
            }
            TAtomic::TResource | TAtomic::TClosedResource => true,
            other => atomic_is_stringable(analyzer, other),
        };
        if valid {
            has_valid_or_castable = true;
        } else if invalid_cast.is_none() {
            invalid_cast = Some(atomic.get_id(Some(analyzer.interner)));
        }
    }

    if let Some(invalid_id) = invalid_cast {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            if has_valid_or_castable {
                IssueKind::PossiblyInvalidCast
            } else {
                IssueKind::InvalidCast
            },
            format!("{} cannot be cast to string", invalid_id),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }
}

/// Psalm's PSEUDO_CASTABLE_CLASSES: classes whose instances cast to int/float
/// (SimpleXMLElement, DOMNode, GMP, Decimal\Decimal), including descendants.
fn named_object_is_pseudo_castable(analyzer: &StatementsAnalyzer<'_>, class_name: StrId) -> bool {
    let pseudo_castable = ["SimpleXMLElement", "DOMNode", "GMP", "Decimal\\Decimal"];
    let mut to_check = vec![class_name];
    if let Some(class_info) = analyzer.codebase.get_class(class_name) {
        to_check.extend(class_info.all_parent_classes.iter().copied());
    }
    to_check.iter().any(|candidate| {
        let candidate_name = analyzer.interner.lookup(*candidate);
        let candidate_name = candidate_name.trim_start_matches('\\');
        pseudo_castable
            .iter()
            .any(|pseudo| candidate_name.eq_ignore_ascii_case(pseudo))
    })
}

fn should_emit_invalid_cast_for_named_object(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: StrId,
) -> bool {
    let Some(class_info) = analyzer.codebase.get_class(class_name) else {
        return false;
    };

    if class_info.kind == ClassLikeKind::Interface {
        return false;
    }

    !class_info.methods.contains_key(&StrId::TO_STRING)
}

/// Check if an atomic type can be implicitly converted to a string
/// (scalars, null, mixed, and objects with `__toString`).
pub(crate) fn atomic_is_stringable(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TString
        | TAtomic::TLiteralString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TTruthyString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TDependentGetClass { .. }
        | TAtomic::TDependentGetType { .. }
        | TAtomic::TInt
        | TAtomic::TNonspecificLiteralInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TIntRange { .. }
        | TAtomic::TFloat
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TBool
        | TAtomic::TTrue
        | TAtomic::TFalse
        | TAtomic::TNull
        | TAtomic::TNever
        | TAtomic::TMixed
        | TAtomic::TMixedFromLoopIsset
        | TAtomic::TNonEmptyMixed
        | TAtomic::TNumeric
        | TAtomic::TScalar
        | TAtomic::TNonEmptyScalar
        | TAtomic::TArrayKey => true,
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .is_some_and(|class_info| class_info.methods.contains_key(&StrId::TO_STRING)),
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(|nested| atomic_is_stringable(analyzer, nested)),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_is_stringable(analyzer, as_type),
        _ => false,
    }
}

fn get_to_string_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<TUnion> {
    let TAtomic::TNamedObject { name, .. } = atomic else {
        return None;
    };

    let class_info = analyzer.codebase.get_class(*name)?;
    let method = class_info.methods.get(&StrId::TO_STRING)?;
    Some(
        method
            .return_type
            .clone()
            .or_else(|| method.signature_return_type.clone())
            .unwrap_or_else(TUnion::string),
    )
}

fn union_is_non_empty_string(return_type: &TUnion) -> bool {
    !return_type.types.is_empty()
        && return_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TNonEmptyString
                    | TAtomic::TTruthyString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TLiteralString { .. }
            )
        })
}

/// Check if an operator is a cast operator.
pub fn is_cast_operator(op: &UnaryPrefixOperator) -> bool {
    matches!(
        op,
        UnaryPrefixOperator::IntCast(_, _)
            | UnaryPrefixOperator::IntegerCast(_, _)
            | UnaryPrefixOperator::FloatCast(_, _)
            | UnaryPrefixOperator::DoubleCast(_, _)
            | UnaryPrefixOperator::RealCast(_, _)
            | UnaryPrefixOperator::StringCast(_, _)
            | UnaryPrefixOperator::BinaryCast(_, _)
            | UnaryPrefixOperator::BoolCast(_, _)
            | UnaryPrefixOperator::BooleanCast(_, _)
            | UnaryPrefixOperator::ArrayCast(_, _)
            | UnaryPrefixOperator::ObjectCast(_, _)
            | UnaryPrefixOperator::UnsetCast(_, _)
            | UnaryPrefixOperator::VoidCast(_, _)
    )
}

/// Check if a cast is redundant given the inner type.
fn is_redundant_cast(op: &UnaryPrefixOperator, inner_type: &TUnion) -> bool {
    // Only consider single-type unions for redundant cast detection
    if !inner_type.is_single() {
        return false;
    }

    let inner = match inner_type.get_single() {
        Some(t) => t,
        None => return false,
    };

    match op {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            // Psalm's `(int)` redundancy uses `Union::isInt()`, true for every
            // int atomic — incl. `int<m,n>` (e.g. `positive-int`) and `literal-int`.
            matches!(
                inner,
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TNonspecificLiteralInt
                    | TAtomic::TIntRange { .. }
            )
        }

        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => {
            matches!(inner, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
        }

        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            matches!(inner, TAtomic::TString | TAtomic::TLiteralString { .. })
        }

        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            matches!(inner, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse)
        }

        UnaryPrefixOperator::ArrayCast(_, _) => {
            matches!(inner, TAtomic::TArray { .. })
        }

        UnaryPrefixOperator::ObjectCast(_, _) => {
            matches!(inner, TAtomic::TObject | TAtomic::TNamedObject { .. })
        }

        _ => false,
    }
}
