//! Reconciler macros.
//!
//! Port of Hakana's `reconciler/macros.rs`. `intersect_simple!` encodes the
//! canonical simple-type reconcile: walk the union's atomics, keep subtypes,
//! short-circuit to the assertion's own type when a supertype (e.g. `mixed`,
//! `scalar`) is present, and track `did_remove_type` — then emit
//! `RedundantCondition` (nothing removed, non-equality assertion) or
//! `TypeDoesNotContainType` (nothing kept) *inside* the reconcile, exactly
//! where the verdict is known (Psalm's `triggerIssueForImpossible` placement).
//!
//! pzoom divergence from Hakana's macro: the fall-through arm delegates
//! template params to the intersection machinery so their `as` bounds keep
//! narrowing; Hakana's `TTypeVariable` arm (record a lower bound, keep the
//! variable) is ported alongside it.

#[macro_export]
macro_rules! intersect_simple {
    (
        $(|)? $( $subtype_pattern:pat_param )|+ $( if $subtype_guard: expr )? $(,)?,
        $(|)? $( $supertype_pattern:pat_param )|+ $( if $supertype_guard: expr )? $(,)?,
        $max_type:expr,
        $assertion:expr,
        $existing_var_type:expr,
        $key:expr,
        $negated:expr,
        $analysis_data:expr,
        $analyzer:expr,
        $is_equality:expr,
    ) => {{
        let mut acceptable_types = Vec::new();
        let mut did_remove_type = false;

        for atomic in &$existing_var_type.types {
            if matches!(atomic, $( $subtype_pattern )|+ $( if $subtype_guard )?) {
                acceptable_types.push(atomic.clone());
            } else if matches!(atomic, $( $supertype_pattern )|+ $( if $supertype_guard )?) {
                return with_docblock_from($max_type, $existing_var_type);
            } else if let TAtomic::TTypeVariable { name } = atomic {
                // Hakana: asserting a simple type on a type variable records
                // it as a lower bound and keeps the variable alive.
                if let Some(pzoom_code_info::TypeVariableBounds { lower_bounds, .. }) =
                    $analysis_data.type_variable_bounds.get_mut(name)
                {
                    let bound =
                        pzoom_code_info::TemplateBound::new($max_type.clone(), 0, None, None);
                    lower_bounds.push(bound);
                }

                did_remove_type = true;
                acceptable_types.push(atomic.clone());
            } else if matches!(atomic, TAtomic::TTemplateParam { .. }) {
                did_remove_type = true;
                if let Some(narrowed) = $assertion
                    .get_type()
                    .and_then(|assertion_atomic| {
                        crate::reconciler::assertion_reconciler::intersect_atomic_with_atomic(
                            atomic,
                            assertion_atomic,
                        )
                    })
                {
                    acceptable_types.push(narrowed);
                }
            } else {
                did_remove_type = true;
            }
        }

        if (acceptable_types.is_empty() && !$existing_var_type.is_nothing())
            || (!did_remove_type && !$is_equality && !acceptable_types.is_empty())
        {
            if let Some(key) = $key {
                crate::reconciler::trigger_issue_for_impossible(
                    $analysis_data,
                    $analyzer,
                    $existing_var_type,
                    key,
                    $assertion,
                    !did_remove_type,
                    $negated,
                );
            }
        }

        if !acceptable_types.is_empty() {
            with_docblock_from(TUnion::from_types(acceptable_types), $existing_var_type)
        } else {
            with_docblock_from(TUnion::nothing(), $existing_var_type)
        }
    }}
}
