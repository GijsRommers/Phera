//! Precedence oracle: generates random expressions from literals and all
//! unary, binary, ternary, cast and comparison operators, parses them with
//! Phera, prints the tree back as a fully parenthesised PHP expression, and
//! has real PHP (one process) evaluate both forms. The `var_export` of both
//! must be identical, which proves the parse tree matches PHP's precedence.
//!
//! Needs a `php` binary (override with `PHERA_PHP=/path/to/php`); skipped without
//! one. Uses a fixed-seed hand-written PRNG (no dependencies).

use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use phera_ast::{BinaryOperator, Expression, ExpressionKind, Literal, UnaryOperator};

/// xorshift64*: deterministic, tiny, no dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const COUNT: usize = 4000;
const SEED: u64 = 0x5DEECE66D;

/// Binary operators to generate, with their PHP source text.
const BINARY_OPS: &[(&str, BinaryOperator)] = &[
    ("+", BinaryOperator::Add),
    ("-", BinaryOperator::Subtract),
    ("*", BinaryOperator::Multiply),
    ("/", BinaryOperator::Divide),
    ("%", BinaryOperator::Modulo),
    ("**", BinaryOperator::Power),
    (".", BinaryOperator::Concat),
    ("<<", BinaryOperator::ShiftLeft),
    (">>", BinaryOperator::ShiftRight),
    ("&", BinaryOperator::BitwiseAnd),
    ("|", BinaryOperator::BitwiseOr),
    ("^", BinaryOperator::BitwiseXor),
    ("==", BinaryOperator::Equal),
    ("!=", BinaryOperator::NotEqual),
    ("===", BinaryOperator::Identical),
    ("!==", BinaryOperator::NotIdentical),
    ("<", BinaryOperator::Less),
    ("<=", BinaryOperator::LessEqual),
    (">", BinaryOperator::Greater),
    (">=", BinaryOperator::GreaterEqual),
    ("<=>", BinaryOperator::Spaceship),
    ("&&", BinaryOperator::And),
    ("||", BinaryOperator::Or),
    ("and", BinaryOperator::And),
    ("or", BinaryOperator::Or),
    ("xor", BinaryOperator::Xor),
    ("??", BinaryOperator::NullCoalesce),
    ("|>", BinaryOperator::Pipe),
];

const UNARY_OPS: &[&str] = &["-", "+", "!", "~", "@"];

const CASTS: &[&str] = &[
    "(int)", "(float)", "(string)", "(bool)", "(array)", "(object)",
];

fn literal(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 => "0".into(),
        1 => "1".into(),
        2 => "2".into(),
        3 => "3".into(),
        4 => "7".into(),
        5 => "255".into(),
        6 => "true".into(),
        7 => "false".into(),
        8 => "null".into(),
        _ => match rng.below(5) {
            0 => "'a'".into(),
            1 => "'b'".into(),
            2 => "'ab'".into(),
            3 => "'0'".into(),
            _ => "''".into(),
        },
    }
}

/// What kind of expression the generator produced, so ternaries can be
/// parenthesised when embedded in a tighter-binding context (an
/// unparenthesised ternary in a condition position is a compile error in
/// PHP, not a parse error, so it would not be a fair oracle).
#[derive(Clone, Copy, PartialEq)]
enum Form {
    Plain,
    Ternary,
}

/// Generates a random expression and whether it is a ternary/elvis at the top.
fn gen_expr(rng: &mut Rng, depth: usize, allow_ternary: bool) -> (String, Form) {
    if depth >= 5 || rng.below(3) == 0 {
        return (literal(rng), Form::Plain);
    }

    let embed = |text: String, form: Form| match form {
        Form::Ternary => format!("({text})"),
        Form::Plain => text,
    };

    match rng.below(if allow_ternary { 6 } else { 5 }) {
        0 => {
            let op = UNARY_OPS[rng.below(UNARY_OPS.len())];
            let (operand, form) = gen_expr(rng, depth + 1, allow_ternary);
            (format!("{op} {}", embed(operand, form)), Form::Plain)
        }
        1 => {
            let cast = CASTS[rng.below(CASTS.len())];
            let (operand, form) = gen_expr(rng, depth + 1, allow_ternary);
            (format!("{cast} {}", embed(operand, form)), Form::Plain)
        }
        2 | 3 => {
            let (op, _) = BINARY_OPS[rng.below(BINARY_OPS.len())];
            let (left, left_form) = gen_expr(rng, depth + 1, allow_ternary);
            // The pipe operator's right side must be callable; use an identity
            // arrow function. `instanceof` needs a class name on the right.
            let (right, right_form) = if op == "|>" {
                ("(fn($__p) => $__p)".to_string(), Form::Plain)
            } else if op == "instanceof" {
                ("stdClass".to_string(), Form::Plain)
            } else {
                gen_expr(rng, depth + 1, allow_ternary)
            };
            (
                format!(
                    "{} {op} {}",
                    embed(left, left_form),
                    embed(right, right_form)
                ),
                Form::Plain,
            )
        }
        4 if allow_ternary => {
            let (condition, condition_form) = gen_expr(rng, depth + 1, allow_ternary);
            let (then, then_form) = gen_expr(rng, depth + 1, allow_ternary);
            let (otherwise, otherwise_form) = gen_expr(rng, depth + 1, allow_ternary);
            // A ternary as the condition would be a compile error in PHP;
            // parenthesise it (the then-branch may stay unparenthesised).
            let condition = embed(condition, condition_form);
            if rng.below(4) == 0 {
                (
                    format!("{condition} ?: {}", embed(otherwise, otherwise_form)),
                    Form::Ternary,
                )
            } else {
                (
                    format!(
                        "{condition} ? {} : {}",
                        embed(then, then_form),
                        embed(otherwise, otherwise_form)
                    ),
                    Form::Ternary,
                )
            }
        }
        _ => {
            let (value, form) = gen_expr(rng, depth + 1, allow_ternary);
            (
                format!("{} instanceof stdClass", embed(value, form)),
                Form::Plain,
            )
        }
    }
}

/// Prints a parsed expression back as fully parenthesised PHP.
fn print_expr(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Literal(literal) => match literal {
            Literal::Null => "null".into(),
            Literal::Bool(b) => b.to_string(),
            Literal::Int(i) => i.to_string(),
            Literal::Float(f) => format!("{f:?}"),
            Literal::String(bytes) => print_string(bytes),
        },
        ExpressionKind::Unary { operator, operand } => {
            let op = match operator {
                UnaryOperator::Plus => "+",
                UnaryOperator::Minus => "-",
                UnaryOperator::Not => "!",
                UnaryOperator::BitwiseNot => "~",
                UnaryOperator::ErrorSuppress => "@",
            };
            format!("({op}{})", print_expr(operand))
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            format!(
                "({} {} {})",
                print_expr(left),
                op_text(*operator),
                print_expr(right)
            )
        }
        ExpressionKind::Ternary {
            condition,
            then,
            otherwise,
        } => match then {
            Some(then) => format!(
                "({} ? {} : {})",
                print_expr(condition),
                print_expr(then),
                print_expr(otherwise)
            ),
            None => format!("({} ?: {})", print_expr(condition), print_expr(otherwise)),
        },
        ExpressionKind::Cast { kind, operand } => {
            let cast = match kind {
                phera_ast::CastKind::Int => "(int)",
                phera_ast::CastKind::Float => "(float)",
                phera_ast::CastKind::String => "(string)",
                phera_ast::CastKind::Bool => "(bool)",
                phera_ast::CastKind::Array => "(array)",
                phera_ast::CastKind::Object => "(object)",
                phera_ast::CastKind::Unset => "(unset)",
                phera_ast::CastKind::Void => "(void)",
            };
            format!("({cast}{})", print_expr(operand))
        }
        ExpressionKind::InstanceOf { value, class } => {
            format!("({} instanceof {})", print_expr(value), print_expr(class))
        }
        ExpressionKind::Name(name) => name.text.clone(),
        ExpressionKind::Variable(phera_ast::Variable::Named(name)) => format!("${name}"),
        ExpressionKind::ArrowFunction(arrow) => {
            format!(
                "(fn({}) => {})",
                arrow
                    .parameters
                    .iter()
                    .map(|p| format!("${}", p.name))
                    .collect::<Vec<_>>()
                    .join(", "),
                print_expr(&arrow.body)
            )
        }
        other => panic!("generator produced an unexpected node: {other:?}"),
    }
}

fn op_text(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Modulo => "%",
        BinaryOperator::Power => "**",
        BinaryOperator::Concat => ".",
        BinaryOperator::BitwiseAnd => "&",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::BitwiseXor => "^",
        BinaryOperator::ShiftLeft => "<<",
        BinaryOperator::ShiftRight => ">>",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
        BinaryOperator::Xor => "xor",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Identical => "===",
        BinaryOperator::NotIdentical => "!==",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Spaceship => "<=>",
        BinaryOperator::NullCoalesce => "??",
        BinaryOperator::Pipe => "|>",
    }
}

/// Single-quoted PHP string literal for the bytes (only `\` and `'` escaped).
fn print_string(bytes: &[u8]) -> String {
    let mut out = String::from("'");
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\'' => out.push_str("\\'"),
            _ => out.push(b as char),
        }
    }
    out.push('\'');
    out
}

fn hex_encode(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

fn main() -> ExitCode {
    let Some(php) = phera_extension_api::testing::oracle::find_php() else {
        println!("skipping oracle: PHP not found (set PHERA_PHP=/path/to/php)");
        return ExitCode::SUCCESS;
    };

    let mut rng = Rng(SEED);
    let mut cases: Vec<(String, Option<String>)> = Vec::new(); // (original, parenthesised)

    for _ in 0..COUNT {
        let (original, _) = gen_expr(&mut rng, 0, true);
        let parsed = phera_parser::parse(&format!("<?php {original};"));
        match parsed {
            Ok(program) => {
                let statement = &program.statements[0];
                let phera_ast::StatementKind::Expression(expression) = &statement.kind else {
                    panic!("expected expression statement for {original:?}");
                };
                cases.push((original, Some(print_expr(expression))));
            }
            Err(error) => {
                // Phera rejected it: ask PHP whether it also rejects it.
                cases.push((original, None));
                let _ = error;
            }
        }
    }

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_eval.php");

    let mut child = Command::new(&php)
        .arg("-n")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("cannot start php");

    let mut input = String::new();
    for (original, parenthesised) in &cases {
        input.push_str(&hex_encode(original));
        input.push(' ');
        if let Some(parenthesised) = parenthesised {
            input.push_str(&hex_encode(parenthesised));
        }
        input.push('\n');
    }

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));

    let output = child.wait_with_output().expect("cannot run php");
    let _ = writer.join();

    if !output.status.success() {
        eprintln!(
            "php failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!(
            "php stdout tail: {}",
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .join("\n")
        );
        // Dump the input for reproduction.
        let dump = Path::new(env!("CARGO_TARGET_TMPDIR")).join("oracle-input.txt");
        let mut all = String::new();
        for (original, parenthesised) in &cases {
            all.push_str(original);
            all.push('\t');
            all.push_str(parenthesised.as_deref().unwrap_or(""));
            all.push('\n');
        }
        let _ = std::fs::write(&dump, &all);
        eprintln!("cases dumped to {}", dump.display());
        return ExitCode::FAILURE;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    let mut compared = 0;
    let mut both_reject = 0;
    let mut phera_accepts_php_rejects = 0;
    let mut failures: Vec<String> = Vec::new();

    for (original, parenthesised) in &cases {
        let original_result = lines.next().unwrap_or("");
        let paren_result = lines.next().unwrap_or("");

        if parenthesised.is_none() {
            // Phera rejected the original: PHP must reject it too.
            if original_result.starts_with("ERR") {
                both_reject += 1;
            } else {
                failures.push(format!(
                    "Phera rejected but PHP accepted: {original} (PHP said {original_result})"
                ));
            }
            continue;
        }

        let parenthesised = parenthesised.as_ref().unwrap();

        // A runtime error (division by zero, type error, ...) is fine as long
        // as both forms raise the same one; a *parse* rejection of the
        // original while Phera accepted it is informational.
        compared += 1;
        if original_result != paren_result {
            if original_result.starts_with("ERR ParseError") && paren_result.starts_with("OK") {
                phera_accepts_php_rejects += 1;
            } else {
                failures.push(format!(
                    "MISMATCH original={original} parsed={parenthesised}\n  php(original)={original_result}\n  php(parsed)  ={paren_result}"
                ));
            }
        }
    }

    for failure in failures.iter().take(10) {
        println!("{failure}");
    }
    if failures.len() > 10 {
        println!("... and {} more", failures.len() - 10);
    }

    println!(
        "\nprecedence oracle: {compared} compared, {} mismatched, {} both reject, {} Phera accepts but PHP rejects, {} total",
        failures.len(),
        both_reject,
        phera_accepts_php_rejects,
        cases.len()
    );

    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
