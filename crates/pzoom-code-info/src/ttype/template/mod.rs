//! Hakana's template-inference structures (`hakana-core/src/code_info/ttype/template/mod.rs`).
//!
//! `TemplateResult` captures the outcome of argument analysis with regard to
//! generic parameters: lower bounds (from non-callable templated arguments)
//! and upper bounds (from callable *parameter* positions — given a parameter
//! `callable(T1): void` and an argument `callable(int): void`, `int` is an
//! upper bound for `T1`; callable *return* positions still yield lower
//! bounds).
//!
//! These will replace pzoom's transitional `TemplateMap` everywhere Hakana
//! uses `TemplateResult` (see task notes); `TTypeVariable` bounds reuse
//! `TemplateBound`.

use std::sync::Arc;

use indexmap::IndexMap;
use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::code_location::CodeLocation;
use crate::t_union::TUnion;

/// The entity that declares a template parameter (Hakana's `GenericParent`,
/// from `code_info/lib.rs`). pzoom has no Hack `type` definitions, but keeps
/// the variant for parity with Hakana call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GenericParent {
    ClassLike(StrId),
    FunctionLike(StrId),
    TypeDefinition(StrId),
}

impl GenericParent {
    /// The class-like's name when this parent is a class-like (the only kind
    /// that can appear in `template_extended_params`).
    pub fn classlike_name(&self) -> Option<StrId> {
        match self {
            GenericParent::ClassLike(id) => Some(*id),
            _ => None,
        }
    }

    pub fn to_string(&self, interner: Option<&pzoom_str::Interner>) -> String {
        let render = |id: &StrId| -> String {
            match interner {
                Some(interner) => interner.lookup(*id).to_string(),
                None => format!("{}", id.0),
            }
        };
        match self {
            GenericParent::ClassLike(id) => render(id),
            GenericParent::FunctionLike(id) => format!("fn-{}", render(id)),
            GenericParent::TypeDefinition(id) => format!("type-{}", render(id)),
        }
    }
}

/// Hakana's `TemplateBound`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateBound {
    pub bound_type: TUnion,

    /// The depth at which the template appears in a given type. In
    /// `Foo<T, Bar<T, array<T>>>`, `T` appears at three different depths; the
    /// shallowest appearance takes prominence when inferring `T`.
    pub appearance_depth: usize,

    /// The argument offset where this bound was set. In `Foo<T, string, T>`
    /// the template appears at argument offsets 0 and 2.
    pub arg_offset: Option<usize>,

    /// When non-null, indicates an equality bound (vs a lower or upper bound).
    pub equality_bound_classlike: Option<StrId>,

    pub pos: Option<CodeLocation>,
}

impl TemplateBound {
    pub fn new(
        bound_type: TUnion,
        appearance_depth: usize,
        arg_offset: Option<usize>,
        equality_bound_classlike: Option<StrId>,
    ) -> Self {
        Self {
            bound_type,
            appearance_depth,
            arg_offset,
            equality_bound_classlike,
            pos: None,
        }
    }
}

/// Hakana's `TemplateResult`.
#[derive(Clone, Debug, Default)]
pub struct TemplateResult {
    pub template_types: IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
    pub lower_bounds: IndexMap<StrId, FxHashMap<GenericParent, Vec<TemplateBound>>>,
    pub upper_bounds: IndexMap<StrId, FxHashMap<GenericParent, TemplateBound>>,
    /// When true, bounds must not be updated.
    pub readonly: bool,
    pub upper_bounds_unintersectable_types: Vec<TUnion>,
    /// Appearance depth recorded for newly inserted lower bounds: derived
    /// (e.g. as-clause-mined) bounds insert at depth 1 so direct argument
    /// bounds (depth 0) take precedence in get_relevant_bounds.
    pub bound_insertion_depth: usize,
}

impl TemplateResult {
    pub fn new(
        template_types: IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
        lower_bounds: IndexMap<StrId, FxHashMap<GenericParent, TUnion>>,
    ) -> TemplateResult {
        let mut new_lower_bounds = IndexMap::new();

        for (k, v) in lower_bounds {
            let mut th = FxHashMap::default();

            for (vk, vv) in v {
                th.insert(vk, vec![TemplateBound::new(vv, 0, None, None)]);
            }

            new_lower_bounds.insert(k, th);
        }
        TemplateResult {
            template_types,
            lower_bounds: new_lower_bounds,
            upper_bounds: IndexMap::new(),
            readonly: false,
            upper_bounds_unintersectable_types: Vec::new(),
            bound_insertion_depth: 0,
        }
    }
}

/// Bounds accumulated for a type variable (Hakana's `TypeVariableBounds`,
/// from `analyzer/function_analysis_data.rs`): constraints recorded while a
/// `TAtomic::TTypeVariable` flows through a function body, reconciled against
/// each other at the end of the function (the Hack typechecker's model).
#[derive(Clone, Debug, Default)]
pub struct TypeVariableBounds {
    pub lower_bounds: Vec<TemplateBound>,
    pub upper_bounds: Vec<TemplateBound>,
}

/// Hakana's `get_relevant_bounds`: sort by appearance depth and keep the
/// shallowest run, escaping when depth changes unless an invariant
/// (equality) bound matched at a different argument offset.
pub fn get_relevant_bounds(lower_bounds: &[TemplateBound]) -> Vec<&TemplateBound> {
    if lower_bounds.len() == 1 {
        return vec![&lower_bounds[0]];
    }

    let mut lower_bounds = lower_bounds.iter().collect::<Vec<_>>();
    lower_bounds.sort_by_key(|bound| bound.appearance_depth);

    let mut current_depth = None;
    let mut had_invariant = false;
    let mut last_arg_offset = None;

    let mut applicable_bounds = vec![];

    for template_bound in lower_bounds {
        if let Some(inner) = current_depth {
            if inner != template_bound.appearance_depth && !applicable_bounds.is_empty() {
                if !had_invariant || last_arg_offset == template_bound.arg_offset {
                    // escape switches when matching on invariant generic
                    // params and when matching
                    break;
                }

                current_depth = Some(template_bound.appearance_depth);
            }
        } else {
            current_depth = Some(template_bound.appearance_depth);
        }

        had_invariant = had_invariant || template_bound.equality_bound_classlike.is_some();

        applicable_bounds.push(template_bound);

        last_arg_offset = template_bound.arg_offset;
    }

    applicable_bounds
}

/// Hakana's `get_most_specific_type_from_bounds`.
pub fn get_most_specific_type_from_bounds(lower_bounds: &[TemplateBound]) -> TUnion {
    let relevant_bounds = get_relevant_bounds(lower_bounds);

    if relevant_bounds.is_empty() {
        return TUnion::mixed();
    }

    if relevant_bounds.len() == 1 {
        return relevant_bounds[0].bound_type.clone();
    }

    let mut specific_type = relevant_bounds[0].bound_type.clone();

    for bound in relevant_bounds {
        specific_type = crate::combine_union_types(&specific_type, &bound.bound_type, false);
    }

    specific_type
}

impl TemplateResult {
    /// Hakana's `insert_bound_type`: record a lower bound for
    /// `[param_name][defining_entity]`, generalizing literal scalars.
    pub fn insert_lower_bound(
        &mut self,
        param_name: StrId,
        defining_entity: GenericParent,
        input_type: TUnion,
        appearance_depth: usize,
        arg_offset: Option<usize>,
        pos: Option<CodeLocation>,
    ) {
        self.lower_bounds
            .entry(param_name)
            .or_default()
            .entry(defining_entity)
            .or_default()
            .push(TemplateBound {
                bound_type: input_type,
                appearance_depth,
                arg_offset,
                equality_bound_classlike: None,
                pos,
            });
    }
}
