//! Method visibility checks. Mirrors Psalm `MethodVisibilityAnalyzer`.

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::StrId;

use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

pub(crate) fn receiver_allows_method_visibility_override(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_type: &TUnion,
    target_class: StrId,
) -> bool {
    let mut has_target_class = false;
    let mut has_override_interface = false;

    let mut track_named = |name: StrId| {
        if name == target_class {
            has_target_class = true;
        }

        if analyzer.codebase.get_class(name).is_some_and(|info| {
            info.kind == ClassLikeKind::Interface && info.override_method_visibility
        }) {
            has_override_interface = true;
        }
    };

    for atomic in &receiver_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => track_named(*name),
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if let TAtomic::TNamedObject { name, .. } = nested {
                        track_named(*name);
                    }
                }
            }
            _ => {}
        }
    }

    has_target_class && has_override_interface
}

pub(crate) fn get_method_visibility_scope_class_id(
    class_info: &ClassLikeInfo,
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> StrId {
    class_info
        .appearing_method_ids
        .get(&method_info.name)
        .copied()
        .or(method_info.declaring_class)
        .unwrap_or(class_info.name)
}

pub(crate) fn can_access_protected_member_visibility(
    analyzer: &StatementsAnalyzer<'_>,
    caller_class: StrId,
    visibility_scope_class: StrId,
) -> bool {
    caller_class == visibility_scope_class
        || object_type_comparator::is_class_subtype_of(
            caller_class,
            visibility_scope_class,
            analyzer.codebase,
        )
        || object_type_comparator::is_class_subtype_of(
            visibility_scope_class,
            caller_class,
            analyzer.codebase,
        )
}

pub(crate) fn should_report_private_method_as_undefined(
    analyzer: &StatementsAnalyzer<'_>,
    calling_class: Option<StrId>,
    visibility_scope: StrId,
) -> bool {
    let Some(caller_class) = calling_class else {
        return false;
    };

    if caller_class == visibility_scope {
        return false;
    }

    let caller_is_subclass = analyzer
        .codebase
        .get_class(caller_class)
        .is_some_and(|caller_info| caller_info.all_parent_classes.contains(&visibility_scope));

    if !caller_is_subclass {
        return false;
    }

    analyzer
        .codebase
        .get_class(visibility_scope)
        .is_some_and(|scope_info| {
            scope_info.used_traits.is_empty() && scope_info.trait_method_aliases.is_empty()
        })
}

pub(crate) fn find_private_method_visibility_scope(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> Option<StrId> {
    let method_id = analyzer.interner.intern(method_name);
    let mut current_class = analyzer
        .codebase
        .get_class(class_id)
        .and_then(|class_info| class_info.parent_class);

    while let Some(parent_id) = current_class {
        let parent_info = analyzer.codebase.get_class(parent_id)?;
        if let Some(method_info) = parent_info.methods.get(&method_id)
            && method_info.visibility == Visibility::Private
        {
            return parent_info
                .declaring_method_ids
                .get(&method_id)
                .copied()
                .or(method_info.declaring_class)
                .or(Some(parent_id));
        }

        current_class = parent_info.parent_class;
    }

    None
}
