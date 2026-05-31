//! `sprintf()` / `printf()` return-type provider.
//!
//! Mirrors Psalm's SprintfReturnTypeProvider: validates the format string against
//! the supplied arguments, emitting RedundantFunctionCall / TooFewArguments /
//! TooManyArguments / InvalidArgument. The return type is left to the stub.

use mago_syntax::ast::ast::argument::Argument;
use pzoom_code_info::{
    Issue, IssueKind, TAtomic, TUnion, t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE,
};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) struct SprintfReturnTypeProvider;

impl FunctionReturnTypeProvider for SprintfReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["sprintf", "printf"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        analyze_sprintf_call(
            event.analyzer,
            event.function_id,
            event.args,
            event.arg_positions,
            analysis_data,
        );
        None
    }
}

/// Analyze a call to `sprintf`/`printf` for argument/format mismatches, mirroring
/// Psalm's SprintfReturnTypeProvider. Emits RedundantFunctionCall / TooFewArguments /
/// TooManyArguments / InvalidArgument as appropriate. The return type itself is left
/// to the function stub.
pub(crate) fn analyze_sprintf_call(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(first_pos) = arg_positions.first().copied() else {
        return;
    };
    let func_id = func_name.to_ascii_lowercase();
    let (line, col) = analyzer.get_line_column(first_pos.0);
    let arg_count = args.len();
    let has_splat = args.iter().any(|arg| arg.is_unpacked());

    let emit = |analysis_data: &mut FunctionAnalysisData, kind: IssueKind, message: String| {
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            first_pos.0,
            first_pos.1,
            line,
            col,
        ));
    };

    // sprintf(...$array) with a single splat argument: the splat is redundant.
    if arg_count == 1 && has_splat {
        emit(
            analysis_data,
            IssueKind::RedundantFunctionCall,
            format!(
                "Using the splat operator is redundant, as v{0} without splat operator \
                 can be used instead of {0}",
                func_id
            ),
        );
        return;
    }

    // Extract the format string literal, if any.
    let format = match analysis_data.get_expr_type(first_pos).as_ref() {
        Some(first_type) => match first_type.get_single() {
            Some(TAtomic::TLiteralString { value })
                if value != NON_SPECIFIC_LITERAL_STRING_VALUE =>
            {
                Some(value.clone())
            }
            _ => None,
        },
        None => None,
    };

    // A single non-literal argument has no placeholders to substitute.
    if arg_count == 1 && format.is_none() {
        emit(
            analysis_data,
            IssueKind::RedundantFunctionCall,
            format!(
                "Using {} with a single argument is redundant, since there are no \
                 placeholder params to be substituted",
                func_id
            ),
        );
        return;
    }

    let Some(format) = format else {
        return;
    };

    if format.is_empty() {
        emit(
            analysis_data,
            IssueKind::RedundantFunctionCall,
            format!("Calling {} with an empty first argument does nothing", func_id),
        );
        return;
    }

    if is_always_empty_sprintf_format(&format) {
        emit(
            analysis_data,
            IssueKind::InvalidArgument,
            format!(
                "The pattern of argument 1 of {} will always return an empty string",
                func_id
            ),
        );
        return;
    }

    // Formats with `*` (variable) width/precision are too complex to validate.
    if is_complex_sprintf_format(&format) {
        return;
    }

    let Some(required) = count_sprintf_placeholders(&format) else {
        emit(
            analysis_data,
            IssueKind::InvalidArgument,
            format!("Argument 1 of {} is invalid", func_id),
        );
        return;
    };

    // With a splat argument we cannot statically know how many values are provided.
    if has_splat {
        return;
    }

    let provided = arg_count - 1;

    if required == 0 {
        if arg_count == 1 {
            emit(
                analysis_data,
                IssueKind::RedundantFunctionCall,
                format!(
                    "Using {} with a single argument is redundant, since there are no \
                     placeholder params to be substituted",
                    func_id
                ),
            );
        } else {
            emit(
                analysis_data,
                IssueKind::TooManyArguments,
                format!(
                    "Too many arguments for the number of placeholders in {}",
                    func_id
                ),
            );
        }
        return;
    }

    if provided < required {
        emit(
            analysis_data,
            IssueKind::TooFewArguments,
            format!("Too few arguments for {}", func_id),
        );
    } else if provided > required {
        emit(
            analysis_data,
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments for the number of placeholders in {}",
                func_id
            ),
        );
    }
}

fn is_sprintf_specifier(c: u8) -> bool {
    matches!(
        c,
        b'b' | b'c'
            | b'd'
            | b'e'
            | b'E'
            | b'f'
            | b'F'
            | b'g'
            | b'G'
            | b'h'
            | b'H'
            | b'o'
            | b's'
            | b'u'
            | b'x'
            | b'X'
    )
}

/// Matches Psalm's `/^%(?:\d+\$)?[-+]?0(?:\.0)?s$/`: a format that always produces an
/// empty string (zero-width `%s`).
fn is_always_empty_sprintf_format(format: &str) -> bool {
    let b = format.as_bytes();
    let mut i = 0;
    if i >= b.len() || b[i] != b'%' {
        return false;
    }
    i += 1;

    // optional argnum: \d+\$
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > start && i < b.len() && b[i] == b'$' {
        i += 1;
    } else {
        i = start;
    }

    // [-+]?
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }

    // 0
    if i < b.len() && b[i] == b'0' {
        i += 1;
    } else {
        return false;
    }

    // (?:\.0)?
    if i + 1 < b.len() && b[i] == b'.' && b[i + 1] == b'0' {
        i += 2;
    }

    // s$
    if i < b.len() && b[i] == b's' {
        i += 1;
    } else {
        return false;
    }

    i == b.len()
}

/// Matches Psalm's complex-placeholder regex
/// `%(?:\d+\$)?[-+]?(?:\d+|\*)(?:\.(?:\d+|\*))?[bcdouxXeEfFgGhHs]` anywhere in the format.
/// Such placeholders (notably variable `*` width/precision) are not validated.
fn is_complex_sprintf_format(format: &str) -> bool {
    let b = format.as_bytes();
    (0..b.len()).any(|i| b[i] == b'%' && complex_sprintf_match_at(b, i))
}

fn complex_sprintf_match_at(b: &[u8], start: usize) -> bool {
    let mut i = start + 1;

    // optional argnum: \d+\$
    let argnum_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > argnum_start && i < b.len() && b[i] == b'$' {
        i += 1;
    } else {
        i = argnum_start;
    }

    // [-+]?
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }

    // (?:\d+|\*) - required
    if i < b.len() && b[i] == b'*' {
        i += 1;
    } else {
        let width_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == width_start {
            return false;
        }
    }

    // (?:\.(?:\d+|\*))?
    if i < b.len() && b[i] == b'.' {
        i += 1;
        if i < b.len() && b[i] == b'*' {
            i += 1;
        } else {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }

    i < b.len() && is_sprintf_specifier(b[i])
}

/// Count the number of arguments a (non-complex, non-always-empty) sprintf format
/// requires. Returns None if the format is syntactically invalid (e.g. a stray `%`).
fn count_sprintf_placeholders(format: &str) -> Option<usize> {
    let b = format.as_bytes();
    let mut i = 0;
    let mut sequential = 0usize;
    let mut max_argnum = 0usize;

    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;

        // Literal `%%`.
        if i < b.len() && b[i] == b'%' {
            i += 1;
            continue;
        }

        // optional argnum: \d+\$
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let argnum = if i > start && i < b.len() && b[i] == b'$' {
            let value: usize = format[start..i].parse().ok()?;
            i += 1;
            Some(value)
        } else {
            i = start;
            None
        };

        // flags: - + space 0 and `'<pad char>`
        loop {
            if i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'0') {
                i += 1;
            } else if i < b.len() && b[i] == b'\'' && i + 1 < b.len() {
                i += 2;
            } else {
                break;
            }
        }

        // width
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }

        // precision
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }

        // specifier
        if i < b.len() && is_sprintf_specifier(b[i]) {
            i += 1;
            match argnum {
                Some(n) => max_argnum = max_argnum.max(n),
                None => sequential += 1,
            }
        } else {
            return None;
        }
    }

    Some(max_argnum.max(sequential))
}
