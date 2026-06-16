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

        // printf returns the byte count; only sprintf's string is refined.
        if !event.function_id.eq_ignore_ascii_case("sprintf") {
            return None;
        }

        infer_sprintf_return_type(event.arg_positions, analysis_data)
    }
}

/// Psalm's SprintfReturnTypeProvider return refinement: it runs the real
/// sprintf with empty-string dummies — a non-empty "initial result" (static
/// text, or any specifier that formats '' to a non-empty value like %d → "0")
/// makes the call non-empty-string; a format that is empty with dummy args
/// still returns non-empty-string when some argument is provably non-empty.
fn infer_sprintf_return_type(
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let first_pos = arg_positions.first().copied()?;
    let format = match analysis_data
        .expr_types
        .get(&first_pos)
        .cloned()?
        .get_single()?
    {
        TAtomic::TLiteralString { value } if value != NON_SPECIFIC_LITERAL_STRING_VALUE => {
            value.clone()
        }
        _ => return None,
    };

    if format.is_empty() || is_complex_sprintf_format(&format) {
        return None;
    }

    match sprintf_format_with_empty_args(&format)? {
        FormatEmptiness::NonEmpty => Some(TUnion::new(TAtomic::TNonEmptyString)),
        FormatEmptiness::DependsOnArgs => {
            for arg_pos in arg_positions.iter().skip(1) {
                let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned() else {
                    continue;
                };
                if !arg_type.types.is_empty()
                    && arg_type.types.iter().all(|atomic| {
                        matches!(
                            atomic,
                            TAtomic::TNonEmptyString
                                | TAtomic::TTruthyString
                                | TAtomic::TClassString { .. }
                                | TAtomic::TInt
                                | TAtomic::TLiteralInt { .. }
                                | TAtomic::TIntRange { .. }
                                | TAtomic::TFloat
                                | TAtomic::TLiteralFloat { .. }
                                | TAtomic::TNumeric
                        ) || matches!(
                            atomic,
                            TAtomic::TLiteralString { value } if !value.is_empty()
                        ) || matches!(atomic, TAtomic::TLiteralClassString { .. })
                    })
                {
                    return Some(TUnion::new(TAtomic::TNonEmptyString));
                }
            }
            None
        }
    }
}

enum FormatEmptiness {
    /// Static text or a zero-producing specifier guarantees output.
    NonEmpty,
    /// Only bare `%s` placeholders: emptiness depends on the arguments.
    DependsOnArgs,
}

/// Walk the format like sprintf would with '' for every argument. Returns
/// None when the format is malformed (validation reported it already).
fn sprintf_format_with_empty_args(format: &str) -> Option<FormatEmptiness> {
    let bytes = format.as_bytes();
    let mut i = 0;
    let mut has_static_text = false;
    let mut has_plain_string_placeholder = false;

    while i < bytes.len() {
        if bytes[i] != b'%' {
            has_static_text = true;
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'%' {
            // a literal percent sign
            has_static_text = true;
            i += 1;
            continue;
        }

        // flags / width / precision / argnum
        let spec_start = i;
        let mut width: usize = 0;
        let mut has_precision = false;
        while i < bytes.len() && !is_sprintf_specifier(bytes[i]) {
            match bytes[i] {
                b'.' => has_precision = true,
                b'0'..=b'9' if !has_precision => {
                    width = width * 10 + usize::from(bytes[i] - b'0');
                }
                _ => {}
            }
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let specifier = bytes[i];
        i += 1;

        if specifier == b's' {
            if width > 0 && !has_precision {
                // padding guarantees output ("%5s" with '' is five spaces)
                has_static_text = true;
            } else {
                has_plain_string_placeholder = true;
            }
        } else {
            // numeric/char specifiers format '' to at least one byte
            // ("%d" → "0", "%f" → "0.000000", …)
            has_static_text = true;
        }
        let _ = spec_start;
    }

    if has_static_text {
        Some(FormatEmptiness::NonEmpty)
    } else if has_plain_string_placeholder {
        Some(FormatEmptiness::DependsOnArgs)
    } else {
        None
    }
}

/// Analyze a call to `sprintf`/`printf` for argument/format mismatches, mirroring
/// Psalm's SprintfReturnTypeProvider. Emits RedundantFunctionCall / TooFewArguments /
/// TooManyArguments / InvalidArgument as appropriate. The return type itself is left
/// to the function stub.
fn analyze_sprintf_call(
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
    let format = match analysis_data.expr_types.get(&first_pos).cloned().as_ref() {
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
            format!(
                "Calling {} with an empty first argument does nothing",
                func_id
            ),
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

    // Complex placeholders (e.g. `*` variable width/precision) still fall back to a
    // generic return type, but their format and argument count are validated below,
    // mirroring Psalm's SprintfReturnTypeProvider after vimeo/psalm@6e38dc1.

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

/// Matches Psalm's complex-placeholder regex (since vimeo/psalm@6e38dc1)
/// `%(?:\d+\$)?[-+]?(?:(?:\d+|\*(?:\d+\$)?)(?:\.(?:\d+|\*(?:\d+\$)?))?|\.\*(?:\d+\$)?)[bcdouxXeEfFgGhHs]`
/// anywhere in the format. Such placeholders (notably variable `*` width/precision)
/// have their argument count validated, but the return type is not refined.
fn is_complex_sprintf_format(format: &str) -> bool {
    let b = format.as_bytes();
    (0..b.len()).any(|i| b[i] == b'%' && complex_sprintf_match_at(b, i))
}

/// Skip an optional `\d+\$` group; returns the position after it, or `start`
/// unchanged when the digits are not terminated by `$`.
fn skip_optional_positional_argnum(b: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > start && i < b.len() && b[i] == b'$' {
        i + 1
    } else {
        start
    }
}

fn complex_sprintf_match_at(b: &[u8], start: usize) -> bool {
    let mut i = start + 1;

    // optional argnum: \d+\$
    i = skip_optional_positional_argnum(b, i);

    // [-+]?
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }

    if i >= b.len() {
        return false;
    }

    if b[i] == b'.' {
        // \.\*(?:\d+\$)? — precision-only `*` placeholder
        i += 1;
        if i >= b.len() || b[i] != b'*' {
            return false;
        }
        i += 1;
        i = skip_optional_positional_argnum(b, i);
    } else {
        // (?:\d+|\*(?:\d+\$)?) - required
        if b[i] == b'*' {
            i += 1;
            i = skip_optional_positional_argnum(b, i);
        } else {
            let width_start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == width_start {
                return false;
            }
        }

        // (?:\.(?:\d+|\*(?:\d+\$)?))?
        if i < b.len() && b[i] == b'.' {
            i += 1;
            if i < b.len() && b[i] == b'*' {
                i += 1;
                i = skip_optional_positional_argnum(b, i);
            } else {
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }
    }

    i < b.len() && is_sprintf_specifier(b[i])
}

/// Count the number of arguments a (non-always-empty) sprintf format requires,
/// emulating PHP's `php_formatted_print`. A variable `*` width/precision consumes
/// the next sequential argument (or, as `*N$`, the argument at position N) before
/// the conversion itself does. Returns None if the format is syntactically invalid
/// (e.g. a stray `%`, or `*` digits not terminated by `$`).
fn count_sprintf_placeholders(format: &str) -> Option<usize> {
    let b = format.as_bytes();
    let mut i = 0;
    let mut sequential = 0usize;
    let mut max_argnum = 0usize;

    // Parse a `*` width/precision: a bare `*` consumes the next sequential
    // argument; `*N$` references argument N; `*` followed by digits without a
    // closing `$` is a PHP ValueError ("Unknown format specifier").
    let parse_star =
        |i: &mut usize, sequential: &mut usize, max_argnum: &mut usize| -> Option<()> {
            *i += 1;
            let start = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            if *i > start {
                if *i < b.len() && b[*i] == b'$' {
                    let value: usize = format[start..*i].parse().ok()?;
                    *i += 1;
                    *max_argnum = (*max_argnum).max(value);
                } else {
                    return None;
                }
            } else {
                *sequential += 1;
            }
            Some(())
        };

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

        // width: digits or `*` / `*N$`
        if i < b.len() && b[i] == b'*' {
            parse_star(&mut i, &mut sequential, &mut max_argnum)?;
        } else {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }

        // precision: digits or `*` / `*N$`
        if i < b.len() && b[i] == b'.' {
            i += 1;
            if i < b.len() && b[i] == b'*' {
                parse_star(&mut i, &mut sequential, &mut max_argnum)?;
            } else {
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }

        // specifier; `%` (a literal percent with modifiers, e.g. `%1$%`) is valid
        // and still consumes an argument position in PHP
        if i < b.len() && (is_sprintf_specifier(b[i]) || b[i] == b'%') {
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
