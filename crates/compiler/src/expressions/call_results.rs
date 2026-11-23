//! Which arguments PHP passes "as the result of a call".
//!
//! ```php
//! end(explode(',', $list));   // Notice: Only variables should be passed by reference
//! end(count($list));          // Error: end(): Argument #1 ($array) could not be passed by reference
//! ```
//!
//! A by-reference parameter takes the result of a function call, a method
//! call, `new`, `include` or `eval` with a notice, and rejects every other
//! computed value. What a program sees depends on how PHP 8.5 compiles the
//! argument, so this file follows its compiler: a call that PHP turns into
//! an operation of its own (`strlen($s)`, `count($a)`, `is_array($x)`) or
//! calls without a frame (`in_array`, `trim`, `min`) leaves a plain
//! temporary, and that is rejected like any other computed value.
//!
//! The two lists are PHP 8.5's: the special functions of its compiler and
//! the functions its extensions mark as callable without a frame, each for
//! the argument counts given there.

use phera_ast::{Argument, Arguments, Expression, ExpressionKind, Literal};

use crate::Compiler;
use crate::policies::NameKind;

use super::builtin_format::is_compiled_format;

impl Compiler<'_> {
    /// Whether `expression`, written as an argument, is a value that a
    /// by-reference parameter takes with a notice.
    pub(super) fn is_call_result(&self, expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::MethodCall { .. }
            | ExpressionKind::StaticCall { .. }
            | ExpressionKind::New { .. }
            | ExpressionKind::Include { .. }
            | ExpressionKind::Eval(_) => true,
            ExpressionKind::Call {
                callee, arguments, ..
            } => {
                // `strlen(...)` makes a closure: not a call.
                let Arguments::List(arguments) = arguments else {
                    return false;
                };
                match &callee.kind {
                    ExpressionKind::Name(name) if callee.span == name.span => {
                        let (resolved, fallback) = self.resolve(name, NameKind::Function);
                        // `namespace App; strlen($s)` may mean `App\strlen`,
                        // so PHP compiles it as an ordinary call.
                        fallback.is_some() || !is_compiled_away(&resolved, arguments)
                    }
                    _ => true,
                }
            }
            _ => false,
        }
    }
}

/// Whether PHP 8.5 compiles a positional call of the global function `name`
/// with these arguments into something other than a call with a frame.
fn is_compiled_away(name: &str, arguments: &[Argument]) -> bool {
    if arguments
        .iter()
        .any(|argument| argument.spread || argument.name.is_some())
    {
        return false;
    }
    let literal = |index: usize| match arguments.get(index).map(|argument| &argument.value.kind) {
        Some(ExpressionKind::Literal(literal)) => Some(literal),
        _ => None,
    };
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    match (name.as_str(), arguments.len()) {
        // An operation of its own.
        (
            "strlen" | "is_null" | "is_bool" | "is_long" | "is_int" | "is_integer" | "is_float"
            | "is_double" | "is_string" | "is_array" | "is_object" | "is_resource" | "is_scalar"
            | "boolval" | "intval" | "floatval" | "doubleval" | "strval" | "count" | "sizeof"
            | "gettype",
            1,
        )
        | ("get_class", 0 | 1)
        | ("get_called_class" | "func_num_args" | "func_get_args", 0)
        | ("array_key_exists", 2) => true,
        // Worked out when the file is compiled: a constant's plain name, a
        // byte value, the first byte of a string.
        ("defined", 1) => matches!(literal(0), Some(Literal::String(name))
            if !name.contains(&b'\\') && !name.contains(&b':')),
        ("chr", 1) => matches!(literal(0), Some(Literal::Int(0..=255))),
        ("ord", 1) => matches!(literal(0), Some(Literal::String(bytes)) if !bytes.is_empty()),
        // `array_slice(func_get_args(), 2)` reads the arguments from 2 on.
        ("array_slice", 2) => {
            matches!(literal(1), Some(Literal::Int(offset)) if *offset >= 0)
                && is_plain_call_of(&arguments[0].value, "func_get_args")
        }
        // `sprintf('%s: %d', $a, $b)` becomes a concatenation.
        ("sprintf", 1..) => match literal(0) {
            Some(Literal::String(format)) => is_compiled_format(format, arguments.len() - 1),
            _ => false,
        },
        // Called without a frame.
        ("property_exists" | "preg_match" | "min" | "max", 2)
        | ("class_exists" | "trim" | "implode" | "dirname", 1 | 2)
        | ("in_array" | "strstr" | "strpos" | "substr" | "strtr", 2 | 3)
        | ("str_contains" | "str_starts_with", 2)
        | ("preg_replace" | "str_replace", 3)
        | ("dechex" | "is_numeric", 1) => true,
        _ => false,
    }
}

/// Whether `expression` is `name()` with no arguments, written unqualified
/// or fully qualified.
fn is_plain_call_of(expression: &Expression, name: &str) -> bool {
    match &expression.kind {
        ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
            ..
        } => {
            arguments.is_empty()
                && matches!(&callee.kind, ExpressionKind::Name(written)
                    if written.text.trim_start_matches('\\').eq_ignore_ascii_case(name))
        }
        _ => false,
    }
}
