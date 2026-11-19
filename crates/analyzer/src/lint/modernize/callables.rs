//! Recognize `strpos($text, $needle) !== false` and direct callable conversions.

use super::{
    expressions::{report_replacement, same_variable},
    rule,
};
use crate::lint::{
    Context,
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    Argument, Arguments, BinaryOperator as Op, CastKind, Expression, ExpressionKind as Expr,
    Literal, MemberName, Name, NameKind, StatementKind as Stmt, UnaryOperator, UseKind, Variable,
};

pub(super) const RULES: &[Rule] = &[
    rule(
        "str-contains",
        "Use str_contains() for a strict strpos() comparison with false.",
        "$found = strpos($text, $needle) !== false;",
        "$found = str_contains($text, $needle);",
        "Safe fix for a known built-in and string arguments. With unproven arguments, the unsafe fix can change error and deprecation messages. Namespace fallbacks are suggestions only.",
        Visit::Expression,
        str_contains,
    ),
    rule(
        "str-starts-with",
        "Use str_starts_with() for strpos() compared strictly with zero.",
        "$starts = strpos($text, $needle) === 0;",
        "$starts = str_starts_with($text, $needle);",
        "Safe fix for a known built-in and string arguments. With unproven arguments, the unsafe fix can change error and deprecation messages. Namespace fallbacks are suggestions only.",
        Visit::Expression,
        str_starts,
    ),
    rule(
        "str-ends-with",
        "Use str_ends_with() for a suffix check with substr() and strlen().",
        "$ends = substr($text, -strlen($suffix)) === $suffix;",
        "$ends = str_ends_with($text, $suffix);",
        "Unsafe fix. An empty suffix returns true with str_ends_with().",
        Visit::Expression,
        str_ends,
    ),
    rule(
        "class-name-constant",
        "Use ::class on an object to read its class name.",
        "$name = get_class($object);",
        "$name = $object::class;",
        "Unsafe fix. The diagnostic on a non-object operand can change.",
        Visit::Expression,
        class_constant,
    ),
    rule(
        "first-class-callable",
        "Use first-class callable syntax for Closure::fromCallable() with a function-name literal.",
        "$length = Closure::fromCallable('strlen');",
        "$length = strlen(...);",
        "Unsafe fix. Resolution follows the explicit fully qualified name and failure diagnostics can change.",
        Visit::Expression,
        first_class_callable,
    ),
    rule(
        "arrow-function",
        "Consider an arrow function for a closure that only returns a value.",
        "$f = function () use ($a) { return $a; };",
        "$f = fn () => $a;",
        "",
        Visit::Expression,
        arrow_function,
    ),
    rule(
        "static-closure",
        "Consider a static closure when a closure has no apparent use of $this.",
        "$f = function () { return 1; };",
        "$f = static function () { return 1; };",
        "Adding static changes later rebinding. Review callers that use Closure::bindTo().",
        Visit::Expression,
        static_closure,
    ),
];

fn str_contains(context: &mut Context<'_>, node: Node<'_>) {
    strpos(
        context,
        node,
        Op::NotIdentical,
        |value| matches!(value, Expr::Literal(Literal::Bool(false))),
        "str-contains",
        b"str_contains",
    );
}

fn str_starts(context: &mut Context<'_>, node: Node<'_>) {
    strpos(
        context,
        node,
        Op::Identical,
        |value| matches!(value, Expr::Literal(Literal::Int(0))),
        "str-starts-with",
        b"str_starts_with",
    );
}

fn strpos(
    context: &mut Context<'_>,
    node: Node<'_>,
    comparison: Op,
    literal: impl Fn(&Expr) -> bool,
    rule: &'static str,
    replacement: &[u8],
) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Binary {
        operator,
        left,
        right,
    } = &expr.kind
    else {
        return;
    };
    if *operator != comparison {
        return;
    }
    let call = if literal(&left.kind) {
        right
    } else if literal(&right.kind) {
        left
    } else {
        return;
    };
    let Expr::Call {
        callee,
        arguments: Arguments::List(arguments),
    } = &call.kind
    else {
        return;
    };
    let Expr::Name(name) = &callee.kind else {
        return;
    };
    let [haystack, needle] = arguments.as_slice() else {
        return;
    };
    if arguments
        .iter()
        .any(|argument| argument.name.is_some() || argument.spread)
    {
        return;
    }
    let message = format!(
        "Use {}() for this strpos() comparison.",
        String::from_utf8_lossy(replacement)
    );
    let resolved = context.names.resolve(name, UseKind::Function);
    let is_strpos = |name: &str| name.eq_ignore_ascii_case("strpos");
    match resolved.as_slice() {
        [name] if is_strpos(name) => {
            let strings = string_expression(&haystack.value) && string_expression(&needle.value);
            report_replacement(
                context,
                rule,
                expr.span,
                &message,
                call_pieces(context, replacement, &haystack.value, &needle.value),
                !strings,
            );
        }
        [local, fallback]
            if is_strpos(fallback)
                && !context.index.is_some_and(|index| {
                    index.functions.contains_key(&local.to_ascii_lowercase())
                }) =>
        {
            context.report(
                rule,
                expr.span,
                format!("{message} Confirm this call resolves to the PHP built-in."),
            );
        }
        _ => {}
    }
}

fn string_expression(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        Expr::Literal(Literal::String(_))
            | Expr::InterpolatedString(_)
            | Expr::Cast {
                kind: CastKind::String,
                ..
            }
    )
}

fn str_ends(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Binary {
        operator: Op::Identical,
        left,
        right,
    } = &expr.kind
    else {
        return;
    };
    let Some([haystack, offset]) = args(context, left, "substr") else {
        return;
    };
    let Expr::Unary {
        operator: UnaryOperator::Minus,
        operand,
    } = &offset.value.kind
    else {
        return;
    };
    let Some([needle]) = args(context, operand, "strlen") else {
        return;
    };
    if same_variable(&needle.value, right) {
        report_replacement(
            context,
            "str-ends-with",
            expr.span,
            "Use str_ends_with() for this suffix check.",
            call_pieces(context, b"str_ends_with", &haystack.value, right),
            true,
        );
    }
}

fn class_constant(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Some([object]) = args(context, expr, "get_class") else {
        return;
    };
    report_replacement(
        context,
        "class-name-constant",
        expr.span,
        "Use ::class to read an object's class name.",
        [
            b"(".as_slice(),
            context.source.slice(object.value.span),
            b")::class",
        ],
        true,
    );
}

fn first_class_callable(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::StaticCall {
        class,
        name: MemberName::Identifier(method),
        arguments: Arguments::List(arguments),
    } = &expr.kind
    else {
        return;
    };
    let Expr::Name(class) = &class.kind else {
        return;
    };
    let [argument] = arguments.as_slice() else {
        return;
    };
    let Expr::Literal(Literal::String(function)) = &argument.value.kind else {
        return;
    };
    let function = function.strip_prefix(b"\\").unwrap_or(function);
    if !method.name.eq_ignore_ascii_case("fromCallable")
        || argument.spread
        || argument.name.is_some()
        || function.is_empty()
        || !function
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        || !function
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        || !context
            .names
            .resolve(class, UseKind::Class)
            .first()
            .is_some_and(|class| class.eq_ignore_ascii_case("Closure"))
    {
        return;
    }
    report_replacement(
        context,
        "first-class-callable",
        expr.span,
        "Use first-class callable syntax for this function.",
        [b"\\".as_slice(), function, b"(...)"],
        true,
    );
}

fn arrow_function(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Closure(closure) = &expr.kind else {
        return;
    };
    let [statement] = closure.body.as_slice() else {
        return;
    };
    let Stmt::Return(Some(value)) = &statement.kind else {
        return;
    };
    if closure.by_reference
        || closure.is_generator
        || closure.uses.iter().any(|capture| capture.by_reference)
    {
        return;
    }
    let mut pending = vec![value];
    while let Some(child) = pending.pop() {
        match &child.kind {
            Expr::Literal(_) | Expr::Name(_) => {}
            Expr::Variable(Variable::Named(name))
                if name == "this"
                    || closure
                        .parameters
                        .iter()
                        .any(|parameter| parameter.name == *name)
                    || closure.uses.iter().any(|capture| capture.name == *name) => {}
            Expr::Binary { left, right, .. } => pending.extend([left.as_ref(), right.as_ref()]),
            _ => return,
        }
    }
    context.report(
        "arrow-function",
        expr.span,
        "Consider using an arrow function for this closure.",
    );
}

fn static_closure(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let possible = match &expr.kind {
        Expr::Closure(closure) => !closure.is_static && closure.uses.is_empty(),
        Expr::ArrowFunction(arrow) => !arrow.is_static,
        _ => false,
    };
    let bytes = context.source.slice(expr.span);
    if possible
        && !bytes.windows(5).any(|part| part == b"$this")
        && !bytes
            .windows(7)
            .any(|part| part.eq_ignore_ascii_case(b"static:"))
        && !bytes
            .windows(4)
            .any(|part| part.eq_ignore_ascii_case(b"eval"))
        && !bytes
            .windows(7)
            .any(|part| part.eq_ignore_ascii_case(b"include"))
        && !bytes
            .windows(7)
            .any(|part| part.eq_ignore_ascii_case(b"require"))
    {
        context.report(
            "static-closure",
            expr.span,
            "Consider making this closure static.",
        );
    }
}

fn args<'a>(context: &Context<'_>, call: &'a Expression, expected: &str) -> Option<&'a [Argument]> {
    let Expr::Call {
        callee,
        arguments: Arguments::List(arguments),
    } = &call.kind
    else {
        return None;
    };
    let Expr::Name(name) = &callee.kind else {
        return None;
    };
    if !direct_function(context, name, expected)
        || arguments.iter().any(|arg| arg.name.is_some() || arg.spread)
    {
        return None;
    }
    Some(arguments)
}

pub(super) fn direct_function(context: &Context<'_>, name: &Name, expected: &str) -> bool {
    // In a namespace, an unqualified call can bind to a function supplied by
    // autoload files. Only accept a name that resolves to one direct symbol.
    name.kind == NameKind::FullyQualified && name.text.eq_ignore_ascii_case(expected) || {
        let resolved = context.names.resolve(name, UseKind::Function);
        matches!(resolved.as_slice(), [name] if name.eq_ignore_ascii_case(expected))
    }
}

fn call_pieces<'a>(
    context: &Context<'a>,
    function: &'a [u8],
    first: &Expression,
    second: &Expression,
) -> [&'a [u8]; 7] {
    [
        b"\\",
        function,
        b"(",
        context.source.slice(first.span),
        b", ",
        context.source.slice(second.span),
        b")",
    ]
}
