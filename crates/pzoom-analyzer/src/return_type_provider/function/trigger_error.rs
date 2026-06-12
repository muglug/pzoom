//! `trigger_error` return-type provider (Psalm's
//! `TriggerErrorReturnTypeProvider`, default behaviour).
//!
//! `E_USER_ERROR` terminates the script, so the call returns `never`;
//! warnings/deprecations/notices return `true`; other literal levels return
//! `false` (fatal since PHP 8); unknown levels return `bool`.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

const E_USER_ERROR: i64 = 256;
const E_USER_WARNING: i64 = 512;
const E_USER_NOTICE: i64 = 1024;
const E_USER_DEPRECATED: i64 = 16384;

pub(super) struct TriggerErrorReturnTypeProvider;

impl FunctionReturnTypeProvider for TriggerErrorReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["trigger_error"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let Some(level_pos) = event.arg_positions.get(1) else {
            // The default level is E_USER_NOTICE, so the call returns true.
            return Some(TUnion::new(TAtomic::TTrue));
        };
        let level_type = analysis_data.expr_types.get(&*level_pos).cloned()?;

        let mut return_types = Vec::new();
        for atomic in &level_type.types {
            match atomic {
                TAtomic::TLiteralInt { value }
                    if matches!(*value, E_USER_WARNING | E_USER_NOTICE | E_USER_DEPRECATED) =>
                {
                    return_types.push(TAtomic::TTrue);
                }
                TAtomic::TLiteralInt { value } if *value == E_USER_ERROR => {
                    return_types.push(TAtomic::TNothing);
                }
                TAtomic::TLiteralInt { .. } => {
                    return_types.push(TAtomic::TFalse);
                }
                _ => return_types.push(TAtomic::TBool),
            }
        }

        Some(TUnion::from_types(return_types))
    }
}
