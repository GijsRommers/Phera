//! Unit tests for the parser: every statement and expression kind, the
//! precedence and associativity of every level of PHP's precedence table,
//! and error reporting.
//!
//! Trees are compared through a small s-expression printer, which keeps the
//! assertions readable.

use phera_ast::*;
use phera_parser::{ParseError, parse};

// --- Helpers ---------------------------------------------------------------

/// Parses `<?php ...` source into statements.
fn stmts(src: &str) -> Vec<Statement> {
    parse(&format!("<?php\n{src}"))
        .unwrap_or_else(|e| panic!("parse error in {src:?}: {e}"))
        .statements
}

/// Parses one statement.
fn stmt(src: &str) -> Statement {
    let statements = stmts(src);
    assert_eq!(
        statements.len(),
        1,
        "expected one statement in {src:?}, got {statements:?}"
    );
    statements.into_iter().next().unwrap()
}

/// Parses `<?php <expr>;` and returns the expression.
fn expr(src: &str) -> Expression {
    let statement = stmt(&format!("{src};"));
    match statement.kind {
        StatementKind::Expression(expression) => expression,
        other => panic!("expected an expression statement in {src:?}, got {other:?}"),
    }
}

#[test]
fn anonymous_class_declaration_starts_at_class_keyword() {
    let source = "new\nreadonly\nclass {}";
    let expression = expr(source);
    let ExpressionKind::New {
        class: NewTarget::Anonymous(declaration),
        ..
    } = &expression.kind
    else {
        panic!("expected an anonymous class");
    };
    assert_eq!(
        declaration.span.start - expression.span.start,
        "new\nreadonly\n".len()
    );
}

/// Parses a whole file, tags and all.
fn file(src: &str) -> Program {
    parse(src).unwrap_or_else(|e| panic!("parse error in {src:?}: {e}"))
}

#[test]
fn trait_final_modifier_does_not_become_an_alias() {
    let statement = stmt("class C { use T { T::f as protected final; } }");
    let StatementKind::Class(class) = &statement.kind else {
        panic!("expected a class");
    };
    let ClassMemberKind::TraitUse { adaptations, .. } = &class.members[0].kind else {
        panic!("expected a trait use");
    };
    assert!(matches!(
        adaptations.as_slice(),
        [TraitAdaptation::Alias {
            alias: None,
            visibility: Some(Visibility::Protected),
            is_final: true,
            ..
        }]
    ));
}

#[test]
fn traits_cannot_declare_interfaces() {
    assert!(
        err("trait T implements I {}")
            .message
            .contains("unexpected token \"implements\"")
    );
}

/// The parse error for `<?php ...` source.
fn err(src: &str) -> ParseError {
    parse(&format!("<?php\n{src}")).expect_err(&format!("expected a parse error in {src:?}"))
}

fn sexp_stmt(statement: &Statement) -> String {
    sexp_stmt_kind(&statement.kind)
}

fn sexp_stmts(statements: &[Statement]) -> String {
    format!("({})", sexp_stmt_list(statements))
}

fn sexp_stmt_list(statements: &[Statement]) -> String {
    statements
        .iter()
        .map(sexp_stmt)
        .collect::<Vec<_>>()
        .join(" ")
}

fn sexp_stmt_kind(kind: &StatementKind) -> String {
    match kind {
        StatementKind::Expression(e) => format!("(expr {})", sexp_expr(e)),
        StatementKind::Echo(exprs) => {
            format!(
                "(echo {})",
                exprs.iter().map(sexp_expr).collect::<Vec<_>>().join(" ")
            )
        }
        StatementKind::InlineHtml(html) => format!("(html {html:?})"),
        StatementKind::Block(block) => format!("(block {})", sexp_stmt_list(block)),
        StatementKind::If {
            condition,
            then,
            otherwise,
        } => format!(
            "(if {} {} {})",
            sexp_expr(condition),
            sexp_stmts(then),
            otherwise.as_ref().map_or("none".into(), |b| sexp_stmts(b))
        ),
        StatementKind::While { condition, body } => {
            format!("(while {} {})", sexp_expr(condition), sexp_stmts(body))
        }
        StatementKind::DoWhile { body, condition } => {
            format!("(do {} while {})", sexp_stmts(body), sexp_expr(condition))
        }
        StatementKind::For {
            init,
            condition,
            step,
            body,
        } => format!(
            "(for [{}] [{}] [{}] {})",
            exprs(init),
            exprs(condition),
            exprs(step),
            sexp_stmts(body)
        ),
        StatementKind::Foreach {
            subject,
            key,
            value,
            by_reference,
            body,
        } => format!(
            "(foreach {} key={} value={} ref={} {})",
            sexp_expr(subject),
            key.as_ref().map_or("none".into(), sexp_expr),
            sexp_expr(value),
            by_reference,
            sexp_stmts(body)
        ),
        StatementKind::Switch { subject, cases } => format!(
            "(switch {} {})",
            sexp_expr(subject),
            cases
                .iter()
                .map(|case| format!(
                    "(case {} {})",
                    case.condition.as_ref().map_or("default".into(), sexp_expr),
                    sexp_stmts(&case.body)
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        StatementKind::Break(n) => format!("(break {n})"),
        StatementKind::Continue(n) => format!("(continue {n})"),
        StatementKind::Return(value) => format!(
            "(return {})",
            value.as_ref().map_or("none".into(), sexp_expr)
        ),
        StatementKind::Global(vars) => format!(
            "(global {})",
            vars.iter().map(sexp_variable).collect::<Vec<_>>().join(" ")
        ),
        StatementKind::Static(vars) => format!(
            "(static {})",
            vars.iter()
                .map(|v| format!(
                    "({} {})",
                    v.name,
                    v.default.as_ref().map_or("none".into(), sexp_expr)
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        StatementKind::Unset(vars) => format!("(unset {})", exprs(vars)),
        StatementKind::Try {
            body,
            catches,
            finally,
        } => format!(
            "(try {} [{}] {})",
            sexp_stmts(body),
            catches
                .iter()
                .map(|c| format!(
                    "(catch {} var={} {})",
                    c.types.iter().map(sexp_name).collect::<Vec<_>>().join("|"),
                    c.variable.as_deref().unwrap_or("none"),
                    sexp_stmts(&c.body)
                ))
                .collect::<Vec<_>>()
                .join(" "),
            finally.as_ref().map_or("none".into(), |b| sexp_stmts(b))
        ),
        StatementKind::Declare { directives, body } => format!(
            "(declare {} {})",
            directives
                .iter()
                .map(|d| format!("({}={})", d.name.name, sexp_expr(&d.value)))
                .collect::<Vec<_>>()
                .join(" "),
            body.as_ref().map_or("none".into(), |b| sexp_stmts(b))
        ),
        StatementKind::Goto(name) => format!("(goto {})", name.name),
        StatementKind::Label(name) => format!("(label {})", name.name),
        StatementKind::Namespace {
            name,
            statements,
            braced,
        } => format!(
            "(namespace {} braced={} {})",
            name.as_ref().map_or("none".into(), sexp_name),
            braced,
            sexp_stmts(statements)
        ),
        StatementKind::Use(items) => format!(
            "(use {})",
            items
                .iter()
                .map(|item| format!(
                    "({:?} {} alias={})",
                    item.kind,
                    sexp_name(&item.name),
                    item.alias
                        .as_ref()
                        .map_or("none".into(), |a| a.name.clone())
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        StatementKind::Const { attributes, items } => format!(
            "(const attrs={} {})",
            attributes.len(),
            items
                .iter()
                .map(|item| format!("({} = {})", item.name.name, sexp_expr(&item.value)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        StatementKind::Function(function) => format!("(function {})", sexp_function(function)),
        StatementKind::Class(class) => format!("(class {})", sexp_class(class)),
        StatementKind::HaltCompiler { data_offset } => format!("(halt {data_offset})"),
    }
}

fn exprs(exprs: &[Expression]) -> String {
    exprs.iter().map(sexp_expr).collect::<Vec<_>>().join(" ")
}

fn sexp_expr(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Literal(literal) => sexp_literal(literal),
        ExpressionKind::InterpolatedString(parts) => format!(
            "(interp {})",
            parts
                .iter()
                .map(|part| match part {
                    StringPart::Literal(bytes) =>
                        format!("lit {:?}", String::from_utf8_lossy(bytes)),
                    StringPart::Expression(e) => sexp_expr(e),
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ExpressionKind::Variable(variable) => format!("(var {})", sexp_variable(variable)),
        ExpressionKind::Name(name) => format!("(name {})", sexp_name(name)),
        ExpressionKind::MagicConstant(constant) => format!("(magic {constant:?})"),
        ExpressionKind::Array(items) => format!(
            "(array {})",
            items
                .iter()
                .map(sexp_array_item)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ExpressionKind::List(items) => format!(
            "(list {})",
            items
                .iter()
                .map(|item| item.as_ref().map_or("hole".into(), sexp_array_item))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ExpressionKind::Index { target, index } => format!(
            "(index {} {})",
            sexp_expr(target),
            index.as_ref().map_or("append".into(), |e| sexp_expr(e))
        ),
        ExpressionKind::Property {
            target,
            name,
            nullsafe,
        } => format!(
            "(prop{} {} {})",
            if *nullsafe { "?" } else { "" },
            sexp_expr(target),
            sexp_member(name)
        ),
        ExpressionKind::StaticProperty { class, name } => {
            format!("(staticprop {} {})", sexp_expr(class), sexp_variable(name))
        }
        ExpressionKind::ClassConstant { class, name } => {
            format!("(classconst {} {})", sexp_expr(class), sexp_member(name))
        }
        ExpressionKind::Call { callee, arguments } => {
            format!("(call {} {})", sexp_expr(callee), sexp_arguments(arguments))
        }
        ExpressionKind::MethodCall {
            target,
            name,
            arguments,
            nullsafe,
        } => format!(
            "(methodcall{} {} {} {})",
            if *nullsafe { "?" } else { "" },
            sexp_expr(target),
            sexp_member(name),
            sexp_arguments(arguments)
        ),
        ExpressionKind::StaticCall {
            class,
            name,
            arguments,
        } => format!(
            "(staticcall {} {} {})",
            sexp_expr(class),
            sexp_member(name),
            sexp_arguments(arguments)
        ),
        ExpressionKind::New { class, arguments } => format!(
            "(new {} {})",
            match class {
                NewTarget::Class(e) => sexp_expr(e),
                NewTarget::Anonymous(c) => format!("(anon {})", sexp_class(c)),
            },
            sexp_arguments(arguments)
        ),
        ExpressionKind::Unary { operator, operand } => {
            format!("({:?} {})", operator, sexp_expr(operand))
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            format!("({:?} {} {})", operator, sexp_expr(left), sexp_expr(right))
        }
        ExpressionKind::Assign { target, value } => {
            format!("(= {} {})", sexp_expr(target), sexp_expr(value))
        }
        ExpressionKind::AssignReference { target, value } => {
            format!("(=ref {} {})", sexp_expr(target), sexp_expr(value))
        }
        ExpressionKind::CompoundAssign {
            operator,
            target,
            value,
        } => {
            format!(
                "(cmp{:?} {} {})",
                operator,
                sexp_expr(target),
                sexp_expr(value)
            )
        }
        ExpressionKind::IncDec {
            target,
            increment,
            prefix,
        } => format!(
            "(incdec {} inc={} prefix={})",
            sexp_expr(target),
            increment,
            prefix
        ),
        ExpressionKind::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "(ternary {} {} {})",
            sexp_expr(condition),
            then.as_ref().map_or("none".into(), |e| sexp_expr(e)),
            sexp_expr(otherwise)
        ),
        ExpressionKind::Cast { kind, operand } => {
            format!("(cast {:?} {})", kind, sexp_expr(operand))
        }
        ExpressionKind::InstanceOf { value, class } => {
            format!("(instanceof {} {})", sexp_expr(value), sexp_expr(class))
        }
        ExpressionKind::Isset(vars) => format!("(isset {})", exprs(vars)),
        ExpressionKind::Empty(inner) => format!("(empty {})", sexp_expr(inner)),
        ExpressionKind::Include { kind, path } => {
            format!("(include {:?} {})", kind, sexp_expr(path))
        }
        ExpressionKind::Eval(inner) => format!("(eval {})", sexp_expr(inner)),
        ExpressionKind::Exit(value) => format!(
            "(exit {})",
            value.as_ref().map_or("none".into(), |e| sexp_expr(e))
        ),
        ExpressionKind::Print(inner) => format!("(print {})", sexp_expr(inner)),
        ExpressionKind::Clone { object, with } => format!(
            "(clone {} with={})",
            sexp_expr(object),
            with.as_ref().map_or("none".into(), |e| sexp_expr(e))
        ),
        ExpressionKind::Throw(inner) => format!("(throw {})", sexp_expr(inner)),
        ExpressionKind::Yield { key, value } => format!(
            "(yield key={} value={})",
            key.as_ref().map_or("none".into(), |e| sexp_expr(e)),
            value.as_ref().map_or("none".into(), |e| sexp_expr(e))
        ),
        ExpressionKind::YieldFrom(inner) => format!("(yieldfrom {})", sexp_expr(inner)),
        ExpressionKind::Closure(closure) => format!("(closure {})", sexp_closure(closure)),
        ExpressionKind::ArrowFunction(arrow) => format!("(arrow {})", sexp_arrow(arrow)),
        ExpressionKind::Match(match_) => format!(
            "(match {} {})",
            sexp_expr(&match_.subject),
            match_
                .arms
                .iter()
                .map(|arm| format!(
                    "(arm {} => {})",
                    arm.conditions
                        .as_ref()
                        .map_or("default".into(), |conds| exprs(conds)),
                    sexp_expr(&arm.body)
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn sexp_literal(literal: &Literal) -> String {
    match literal {
        Literal::Null => "null".into(),
        Literal::Bool(b) => format!("bool {b}"),
        Literal::Int(i) => format!("(int {i})"),
        Literal::Float(f) => format!("(float {f})"),
        Literal::String(bytes) => format!("(str {:?})", String::from_utf8_lossy(bytes)),
    }
}

fn sexp_variable(variable: &Variable) -> String {
    match variable {
        Variable::Named(name) => format!("${name}"),
        Variable::Dynamic(e) => format!("(dyn {})", sexp_expr(e)),
    }
}

fn sexp_member(name: &MemberName) -> String {
    match name {
        MemberName::Identifier(identifier) => identifier.name.clone(),
        MemberName::Expression(e) => format!("{{{}}}", sexp_expr(e)),
    }
}

fn sexp_name(name: &Name) -> String {
    format!("{:?}:{}", name.kind, name.text)
}

fn sexp_arguments(arguments: &Arguments) -> String {
    match arguments {
        Arguments::List(args) => args
            .iter()
            .map(|arg| {
                format!(
                    "(arg name={} spread={} {})",
                    arg.name.as_ref().map_or("none".into(), |n| n.name.clone()),
                    arg.spread,
                    sexp_expr(&arg.value)
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
        Arguments::FirstClassCallable => "fcc".into(),
    }
}

fn sexp_array_item(item: &ArrayItem) -> String {
    format!(
        "(item key={} ref={} spread={} {})",
        item.key.as_ref().map_or("none".into(), sexp_expr),
        item.by_reference,
        item.spread,
        sexp_expr(&item.value)
    )
}

fn sexp_type(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Named(name) => format!("Named:{}", sexp_name(name)),
        TypeKind::Nullable(inner) => format!("(? {})", sexp_type(inner)),
        TypeKind::Union(types) => format!(
            "(union {})",
            types.iter().map(sexp_type).collect::<Vec<_>>().join(" ")
        ),
        TypeKind::Intersection(types) => format!(
            "(inter {})",
            types.iter().map(sexp_type).collect::<Vec<_>>().join(" ")
        ),
    }
}

fn sexp_parameter(parameter: &Parameter) -> String {
    format!(
        "(param attrs={} mods={:?} ty={} ref={} variadic={} ${} default={} hooks={})",
        parameter.attributes.len(),
        parameter.modifiers,
        parameter.ty.as_ref().map_or("none".into(), sexp_type),
        parameter.by_reference,
        parameter.variadic,
        parameter.name,
        parameter.default.as_ref().map_or("none".into(), sexp_expr),
        parameter.hooks.len()
    )
}

fn sexp_function(function: &FunctionDeclaration) -> String {
    format!(
        "(attrs={} ref={} {} ({}) : {} {})",
        function.attributes.len(),
        function.by_reference,
        function.name.name,
        function
            .parameters
            .iter()
            .map(sexp_parameter)
            .collect::<Vec<_>>()
            .join(" "),
        function
            .return_type
            .as_ref()
            .map_or("none".into(), sexp_type),
        sexp_stmts(&function.body)
    )
}

fn sexp_closure(closure: &Closure) -> String {
    format!(
        "(attrs={} static={} ref={} ({}) uses=[{}] : {} {})",
        closure.attributes.len(),
        closure.is_static,
        closure.by_reference,
        closure
            .parameters
            .iter()
            .map(sexp_parameter)
            .collect::<Vec<_>>()
            .join(" "),
        closure
            .uses
            .iter()
            .map(|u| format!("{}${}", if u.by_reference { "&" } else { "" }, u.name))
            .collect::<Vec<_>>()
            .join(" "),
        closure
            .return_type
            .as_ref()
            .map_or("none".into(), sexp_type),
        sexp_stmts(&closure.body)
    )
}

fn sexp_arrow(arrow: &ArrowFunction) -> String {
    format!(
        "(attrs={} static={} ref={} ({}) : {} => {})",
        arrow.attributes.len(),
        arrow.is_static,
        arrow.by_reference,
        arrow
            .parameters
            .iter()
            .map(sexp_parameter)
            .collect::<Vec<_>>()
            .join(" "),
        arrow.return_type.as_ref().map_or("none".into(), sexp_type),
        sexp_expr(&arrow.body)
    )
}

fn sexp_class(class: &ClassDeclaration) -> String {
    format!(
        "(attrs={} kind={:?} mods={:?} name={} extends=[{}] implements=[{}] backing={} [{}])",
        class.attributes.len(),
        class.kind,
        class.modifiers,
        class
            .name
            .as_ref()
            .map_or("none".into(), |n| n.name.clone()),
        class
            .extends
            .iter()
            .map(sexp_name)
            .collect::<Vec<_>>()
            .join(" "),
        class
            .implements
            .iter()
            .map(sexp_name)
            .collect::<Vec<_>>()
            .join(" "),
        class.backing_type.as_ref().map_or("none".into(), sexp_type),
        class
            .members
            .iter()
            .map(sexp_member_kind)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn sexp_member_kind(member: &ClassMember) -> String {
    match &member.kind {
        ClassMemberKind::Constant {
            attributes,
            modifiers,
            ty,
            items,
        } => format!(
            "(const attrs={} mods={:?} ty={} [{}])",
            attributes.len(),
            modifiers,
            ty.as_ref().map_or("none".into(), sexp_type),
            items
                .iter()
                .map(|i| format!("({} = {})", i.name.name, sexp_expr(&i.value)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ClassMemberKind::Property {
            attributes,
            modifiers,
            ty,
            items,
        } => format!(
            "(prop attrs={} mods={:?} ty={} [{}])",
            attributes.len(),
            modifiers,
            ty.as_ref().map_or("none".into(), sexp_type),
            items
                .iter()
                .map(|i| format!(
                    "(${}{} hooks=[{}])",
                    i.name,
                    i.default
                        .as_ref()
                        .map_or("".into(), |e| format!(" = {}", sexp_expr(e))),
                    i.hooks.iter().map(sexp_hook).collect::<Vec<_>>().join(" ")
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ClassMemberKind::Method(method) => format!(
            "(method attrs={} mods={:?} ref={} {} ({}) : {} body={})",
            method.attributes.len(),
            method.modifiers,
            method.by_reference,
            method.name.name,
            method
                .parameters
                .iter()
                .map(sexp_parameter)
                .collect::<Vec<_>>()
                .join(" "),
            method.return_type.as_ref().map_or("none".into(), sexp_type),
            method
                .body
                .as_ref()
                .map_or("none".into(), |b| sexp_stmts(b))
        ),
        ClassMemberKind::TraitUse {
            traits,
            adaptations,
        } => format!(
            "(traituse [{}] [{}])",
            traits.iter().map(sexp_name).collect::<Vec<_>>().join(" "),
            adaptations
                .iter()
                .map(|a| match a {
                    TraitAdaptation::Precedence {
                        trait_name,
                        method,
                        instead_of,
                    } => format!(
                        "(prec {}::{} insteadof [{}])",
                        sexp_name(trait_name),
                        method.name,
                        instead_of
                            .iter()
                            .map(sexp_name)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    TraitAdaptation::Alias {
                        trait_name,
                        method,
                        alias,
                        visibility,
                        ..
                    } => format!(
                        "(alias {}::{} as={} vis={:?})",
                        trait_name.as_ref().map_or("none".into(), sexp_name),
                        method.name,
                        alias.as_ref().map_or("none".into(), |a| a.name.clone()),
                        visibility
                    ),
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ClassMemberKind::EnumCase {
            attributes,
            name,
            value,
        } => format!(
            "(case attrs={} {} = {})",
            attributes.len(),
            name.name,
            value.as_ref().map_or("none".into(), sexp_expr)
        ),
    }
}

fn sexp_hook(hook: &PropertyHook) -> String {
    format!(
        "(hook attrs={} final={} ref={} {} ({}) {})",
        hook.attributes.len(),
        hook.is_final,
        hook.by_reference,
        hook.name.name,
        hook.parameters
            .iter()
            .map(sexp_parameter)
            .collect::<Vec<_>>()
            .join(" "),
        match &hook.body {
            HookBody::None => "none".into(),
            HookBody::Expression(e) => format!("=> {}", sexp_expr(e)),
            HookBody::Block(b) => sexp_stmts(b),
        }
    )
}

// --- Expression tests --------------------------------------------------------

#[test]
fn literals() {
    assert_eq!(sexp_expr(&expr("42")), "(int 42)");
    assert_eq!(sexp_expr(&expr("0x1F")), "(int 31)");
    assert_eq!(sexp_expr(&expr("0b101")), "(int 5)");
    assert_eq!(sexp_expr(&expr("0o17")), "(int 15)");
    assert_eq!(sexp_expr(&expr("0777")), "(int 511)");
    assert_eq!(sexp_expr(&expr("1_000_000")), "(int 1000000)");
    assert_eq!(sexp_expr(&expr("0xdead_beef")), "(int 3735928559)");
    // Too big for i64 becomes a float, as in PHP.
    assert_eq!(
        expr("9223372036854775808").kind,
        ExpressionKind::Literal(Literal::Float(9223372036854775808.0))
    );
    assert_eq!(
        sexp_expr(&expr("0xFFFFFFFFFFFFFFFFF")),
        "(float 295147905179352830000)"
    );
    assert_eq!(sexp_expr(&expr("1.5")), "(float 1.5)");
    assert_eq!(sexp_expr(&expr(".5")), "(float 0.5)");
    assert_eq!(sexp_expr(&expr("5.")), "(float 5)");
    assert_eq!(sexp_expr(&expr("1e3")), "(float 1000)");
    assert_eq!(sexp_expr(&expr("1.5e-2")), "(float 0.015)");
    assert_eq!(sexp_expr(&expr("true")), "bool true");
    assert_eq!(sexp_expr(&expr("FALSE")), "bool false");
    assert_eq!(sexp_expr(&expr("null")), "null");
    assert_eq!(sexp_expr(&expr("'abc'")), "(str \"abc\")");
    assert_eq!(sexp_expr(&expr("\"abc\"")), "(str \"abc\")");
    assert_eq!(sexp_expr(&expr("b'b'")), "(str \"b\")");
    // Single quotes: only \\ and \' are escapes.
    assert_eq!(sexp_expr(&expr(r"'a\\b\'c\n'")), "(str \"a\\\\b'c\\\\n\")");
    // Double quotes: the full escape set. Compare raw bytes directly.
    let e = expr(r#""a\nb\t\r\v\e\f\\\$\"\x41\101\7""#);
    let ExpressionKind::Literal(Literal::String(bytes)) = &e.kind else {
        panic!()
    };
    assert_eq!(
        bytes.as_slice(),
        b"a\nb\t\r\x0b\x1b\x0c\\$\"AA\x07".as_slice()
    );
    let e = expr(r#""\400\777""#);
    let ExpressionKind::Literal(Literal::String(bytes)) = &e.kind else {
        panic!()
    };
    assert_eq!(bytes.as_slice(), [0, 0xFF].as_slice());
    assert_eq!(sexp_expr(&expr(r#""\u{1F600}""#)), "(str \"\u{1f600}\")");
    assert_eq!(sexp_expr(&expr(r#""\q\8""#)), "(str \"\\\\q\\\\8\")");
    assert_eq!(sexp_expr(&expr(r#""A""#)), "(str \"A\")");
    // -9223372036854775808 is unary minus applied to a float in PHP.
    match &expr("-9223372036854775808").kind {
        ExpressionKind::Unary {
            operator: UnaryOperator::Minus,
            operand,
        } => assert_eq!(
            operand.kind,
            ExpressionKind::Literal(Literal::Float(9223372036854775808.0))
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn magic_constants() {
    assert_eq!(sexp_expr(&expr("__LINE__")), "(magic Line)");
    assert_eq!(sexp_expr(&expr("__FILE__")), "(magic File)");
    assert_eq!(sexp_expr(&expr("__DIR__")), "(magic Dir)");
    assert_eq!(sexp_expr(&expr("__CLASS__")), "(magic Class)");
    assert_eq!(sexp_expr(&expr("__FUNCTION__")), "(magic Function)");
    assert_eq!(sexp_expr(&expr("__METHOD__")), "(magic Method)");
    assert_eq!(sexp_expr(&expr("__NAMESPACE__")), "(magic Namespace)");
    assert_eq!(sexp_expr(&expr("__TRAIT__")), "(magic Trait)");
    assert_eq!(sexp_expr(&expr("__PROPERTY__")), "(magic Property)");
}

#[test]
fn names_and_variables() {
    assert_eq!(sexp_expr(&expr("Foo")), "(name Unqualified:Foo)");
    assert_eq!(sexp_expr(&expr("Foo\\Bar")), "(name Qualified:Foo\\Bar)");
    assert_eq!(
        sexp_expr(&expr("\\Foo\\Bar")),
        "(name FullyQualified:Foo\\Bar)"
    );
    assert_eq!(sexp_expr(&expr("namespace\\Foo")), "(name Relative:Foo)");
    assert_eq!(sexp_expr(&expr("$a")), "(var $a)");
    assert_eq!(sexp_expr(&expr("$this")), "(var $this)");
    assert_eq!(sexp_expr(&expr("$$a")), "(var (dyn (var $a)))");
    assert_eq!(sexp_expr(&expr("${'a'}")), "(var (dyn (str \"a\")))");
}

#[test]
fn postfix_chains() {
    assert_eq!(sexp_expr(&expr("$a[0]")), "(index (var $a) (int 0))");
    assert_eq!(sexp_expr(&expr("$a[]")), "(index (var $a) append)");
    assert_eq!(sexp_expr(&expr("$a->b")), "(prop (var $a) b)");
    assert_eq!(sexp_expr(&expr("$a?->b")), "(prop? (var $a) b)");
    assert_eq!(
        sexp_expr(&expr("$a->b->c[0]")),
        "(index (prop (prop (var $a) b) c) (int 0))"
    );
    assert_eq!(sexp_expr(&expr("$a->$b")), "(prop (var $a) {(var $b)})");
    assert_eq!(
        sexp_expr(&expr("$a->{$b . 'c'}")),
        "(prop (var $a) {(Concat (var $b) (str \"c\"))})"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::BAR")),
        "(classconst (name Unqualified:Foo) BAR)"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::class")),
        "(classconst (name Unqualified:Foo) class)"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::$bar")),
        "(staticprop (name Unqualified:Foo) $bar)"
    );
    assert_eq!(
        sexp_expr(&expr("$obj::CONST")),
        "(classconst (var $obj) CONST)"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::{$expr}")),
        "(classconst (name Unqualified:Foo) {(var $expr)})"
    );
    assert_eq!(
        sexp_expr(&expr("foo(1)")),
        "(call (name Unqualified:foo) (arg name=none spread=false (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("$a->b(1)")),
        "(methodcall (var $a) b (arg name=none spread=false (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("$a?->b(1)")),
        "(methodcall? (var $a) b (arg name=none spread=false (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::bar(1)")),
        "(staticcall (name Unqualified:Foo) bar (arg name=none spread=false (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("$a::$b(1)")),
        "(staticcall (var $a) {(var $b)} (arg name=none spread=false (int 1)))"
    );
    // Keywords are fine as member names.
    assert_eq!(sexp_expr(&expr("$a->list")), "(prop (var $a) list)");
    assert_eq!(
        sexp_expr(&expr("Foo::new()")),
        "(staticcall (name Unqualified:Foo) new )"
    );
    assert_eq!(
        sexp_expr(&expr("$a->__halt_compiler")),
        "(prop (var $a) __halt_compiler)"
    );
    assert_eq!(
        sexp_expr(&expr("$a->public(set)")),
        "(prop (var $a) public(set))"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::__LINE__")),
        "(classconst (name Unqualified:Foo) __LINE__)"
    );
    assert_eq!(
        sexp_expr(&expr("Foo::true")),
        "(classconst (name Unqualified:Foo) true)"
    );
    assert_eq!(
        sexp_expr(&expr("$a++")),
        "(incdec (var $a) inc=true prefix=false)"
    );
    assert_eq!(
        sexp_expr(&expr("--$a")),
        "(incdec (var $a) inc=false prefix=true)"
    );
}

#[test]
fn arguments() {
    assert_eq!(sexp_expr(&expr("foo()")), "(call (name Unqualified:foo) )");
    assert_eq!(
        sexp_expr(&expr("foo(1, 2)")),
        "(call (name Unqualified:foo) (arg name=none spread=false (int 1)) (arg name=none spread=false (int 2)))"
    );
    assert_eq!(
        sexp_expr(&expr("foo(a: 1, b: 2)")),
        "(call (name Unqualified:foo) (arg name=a spread=false (int 1)) (arg name=b spread=false (int 2)))"
    );
    assert_eq!(
        sexp_expr(&expr("foo(array: 1)")),
        "(call (name Unqualified:foo) (arg name=array spread=false (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("foo(...$args)")),
        "(call (name Unqualified:foo) (arg name=none spread=true (var $args)))"
    );
    assert_eq!(
        sexp_expr(&expr("foo(...)")),
        "(call (name Unqualified:foo) fcc)"
    );
    assert_eq!(
        sexp_expr(&expr("$a->b(...)")),
        "(methodcall (var $a) b fcc)"
    );
    assert_eq!(
        sexp_expr(&expr("A::b(...)")),
        "(staticcall (name Unqualified:A) b fcc)"
    );
    assert_eq!(
        sexp_expr(&expr("foo(1,)")),
        "(call (name Unqualified:foo) (arg name=none spread=false (int 1)))"
    );
}

#[test]
fn arrays_and_lists() {
    assert_eq!(
        sexp_expr(&expr("[1, 'a' => 2, ...$rest]")),
        "(array (item key=none ref=false spread=false (int 1)) (item key=(str \"a\") ref=false spread=false (int 2)) (item key=none ref=false spread=true (var $rest)))"
    );
    assert_eq!(sexp_expr(&expr("array()")), "(array )");
    assert_eq!(
        sexp_expr(&expr("[&$a, 'k' => &$b]")),
        "(array (item key=none ref=true spread=false (var $a)) (item key=(str \"k\") ref=true spread=false (var $b)))"
    );
    assert_eq!(
        sexp_expr(&expr("[$a, , $b] = $x")),
        "(= (list (item key=none ref=false spread=false (var $a)) hole (item key=none ref=false spread=false (var $b))) (var $x))"
    );
    assert_eq!(
        sexp_expr(&expr("list('k' => $v) = $x")),
        "(= (list (item key=(str \"k\") ref=false spread=false (var $v))) (var $x))"
    );
    assert_eq!(
        sexp_expr(&expr("['k' => [$a, $b]] = $x")),
        "(= (list (item key=(str \"k\") ref=false spread=false (list (item key=none ref=false spread=false (var $a)) (item key=none ref=false spread=false (var $b))))) (var $x))"
    );
}

#[test]
fn assignments() {
    assert_eq!(sexp_expr(&expr("$a = 1")), "(= (var $a) (int 1))");
    assert_eq!(
        sexp_expr(&expr("$a = $b = 1")),
        "(= (var $a) (= (var $b) (int 1)))"
    );
    assert_eq!(sexp_expr(&expr("$a = &$b")), "(=ref (var $a) (var $b))");
    assert_eq!(
        sexp_expr(&expr("$a = &$b[0]")),
        "(=ref (var $a) (index (var $b) (int 0)))"
    );
    assert_eq!(sexp_expr(&expr("$a += 1")), "(cmpAdd (var $a) (int 1))");
    assert_eq!(sexp_expr(&expr("$a **= 2")), "(cmpPower (var $a) (int 2))");
    assert_eq!(
        sexp_expr(&expr("$a ??= 1")),
        "(cmpNullCoalesce (var $a) (int 1))"
    );
    assert_eq!(
        sexp_expr(&expr("$a <<= 2")),
        "(cmpShiftLeft (var $a) (int 2))"
    );
    assert_eq!(
        sexp_expr(&expr("$a[0]->b .= 'x'")),
        "(cmpConcat (prop (index (var $a) (int 0)) b) (str \"x\"))"
    );
    assert_eq!(
        sexp_expr(&expr("foo() = 1")),
        "(= (call (name Unqualified:foo) ) (int 1))"
    );
}

#[test]
fn new_expressions() {
    assert_eq!(
        sexp_expr(&expr("new Foo(1)")),
        "(new (name Unqualified:Foo) (arg name=none spread=false (int 1)))"
    );
    assert_eq!(sexp_expr(&expr("new Foo")), "(new (name Unqualified:Foo) )");
    assert_eq!(
        sexp_expr(&expr("new \\Foo\\Bar")),
        "(new (name FullyQualified:Foo\\Bar) )"
    );
    assert_eq!(
        sexp_expr(&expr("new $class(1)")),
        "(new (var $class) (arg name=none spread=false (int 1)))"
    );
    assert_eq!(sexp_expr(&expr("new $a->b")), "(new (prop (var $a) b) )");
    assert_eq!(
        sexp_expr(&expr("new $a[0]")),
        "(new (index (var $a) (int 0)) )"
    );
    assert_eq!(
        sexp_expr(&expr("new Foo::$x")),
        "(new (staticprop (name Unqualified:Foo) $x) )"
    );
    assert_eq!(
        sexp_expr(&expr("new (expr)()")),
        "(new (name Unqualified:expr) )"
    );
    assert_eq!(
        sexp_expr(&expr("new static")),
        "(new (name Unqualified:static) )"
    );
    // Anonymous class arguments land on the `new`.
    let e = expr("new class(7) {}");
    let ExpressionKind::New { arguments, .. } = &e.kind else {
        panic!()
    };
    assert_eq!(
        sexp_arguments(arguments),
        "(arg name=none spread=false (int 7))"
    );
    let e = expr("new class(1) extends B implements I {}");
    let ExpressionKind::New {
        class: NewTarget::Anonymous(c),
        ..
    } = &e.kind
    else {
        panic!()
    };
    assert!(sexp_class(c).contains("extends=[Unqualified:B] implements=[Unqualified:I]"));
    let e = expr("new #[A] readonly class {}");
    let ExpressionKind::New {
        class: NewTarget::Anonymous(c),
        ..
    } = &e.kind
    else {
        panic!()
    };
    assert!(sexp_class(c).contains("attrs=1"));
    assert!(sexp_class(c).contains("is_readonly: true"));
}

#[test]
fn clone_exit_throw_yield() {
    assert_eq!(sexp_expr(&expr("clone $a")), "(clone (var $a) with=none)");
    assert_eq!(sexp_expr(&expr("clone($a)")), "(clone (var $a) with=none)");
    assert_eq!(
        sexp_expr(&expr("clone($a, ['x' => 1])")),
        "(clone (var $a) with=(array (item key=(str \"x\") ref=false spread=false (int 1))))"
    );
    assert_eq!(sexp_expr(&expr("exit")), "(exit none)");
    assert_eq!(sexp_expr(&expr("exit()")), "(exit none)");
    assert_eq!(sexp_expr(&expr("exit(1)")), "(exit (int 1))");
    assert_eq!(sexp_expr(&expr("die('x')")), "(exit (str \"x\"))");
    assert_eq!(sexp_expr(&expr("throw $e")), "(throw (var $e))");
    assert_eq!(
        sexp_expr(&expr("$x = throw $e")),
        "(= (var $x) (throw (var $e)))"
    );
    assert_eq!(sexp_expr(&expr("yield")), "(yield key=none value=none)");
    assert_eq!(
        sexp_expr(&expr("yield $v")),
        "(yield key=(var $v) value=none)"
    );
    assert_eq!(
        sexp_expr(&expr("yield $k => $v")),
        "(yield key=(var $k) value=(var $v))"
    );
    assert_eq!(
        sexp_expr(&expr("yield from $a + $b")),
        "(yieldfrom (Add (var $a) (var $b)))"
    );
}

#[test]
fn control_expressions() {
    assert_eq!(
        sexp_expr(&expr("isset($a, $b[0], $c->d)")),
        "(isset (var $a) (index (var $b) (int 0)) (prop (var $c) d))"
    );
    assert_eq!(sexp_expr(&expr("empty($a)")), "(empty (var $a))");
    assert_eq!(
        sexp_expr(&expr("include 'x'")),
        "(include Include (str \"x\"))"
    );
    assert_eq!(
        sexp_expr(&expr("include_once 'x'")),
        "(include IncludeOnce (str \"x\"))"
    );
    assert_eq!(
        sexp_expr(&expr("require 'x'")),
        "(include Require (str \"x\"))"
    );
    assert_eq!(
        sexp_expr(&expr("require_once 'x'")),
        "(include RequireOnce (str \"x\"))"
    );
    assert_eq!(sexp_expr(&expr("eval('x')")), "(eval (str \"x\"))");
    assert_eq!(sexp_expr(&expr("print $a")), "(print (var $a))");
    assert_eq!(
        sexp_expr(&expr("$x instanceof Foo")),
        "(instanceof (var $x) (name Unqualified:Foo))"
    );
    assert_eq!(
        sexp_expr(&expr("$x instanceof static")),
        "(instanceof (var $x) (name Unqualified:static))"
    );
    assert_eq!(
        sexp_expr(&expr("$x instanceof $y->cls")),
        "(instanceof (var $x) (prop (var $y) cls))"
    );
    assert_eq!(
        sexp_expr(&expr("match($x) { 1, 2 => 'a', default => 'b', }")),
        "(match (var $x) (arm (int 1) (int 2) => (str \"a\")) (arm default => (str \"b\")))"
    );
}

#[test]
fn closures_and_arrow_functions() {
    assert_eq!(
        sexp_expr(&expr("function ($a) use (&$x, $y): int { return $a; }")),
        "(closure (attrs=0 static=false ref=false ((param attrs=0 mods=Modifiers { visibility: None, set_visibility: None, is_static: false, is_abstract: false, is_final: false, is_readonly: false } ty=none ref=false variadic=false $a default=none hooks=0)) uses=[&$x $y] : Named:Unqualified:int ((return (var $a)))))"
    );
    assert_eq!(
        sexp_expr(&expr("static fn&(): int => 1")),
        "(arrow (attrs=0 static=true ref=true () : Named:Unqualified:int => (int 1)))"
    );
    assert_eq!(
        sexp_expr(&expr("#[A] fn($x) => $x * 2")),
        "(arrow (attrs=1 static=false ref=false ((param attrs=0 mods=Modifiers { visibility: None, set_visibility: None, is_static: false, is_abstract: false, is_final: false, is_readonly: false } ty=none ref=false variadic=false $x default=none hooks=0)) : none => (Multiply (var $x) (int 2))))"
    );
}

#[test]
fn interpolated_strings() {
    assert_eq!(sexp_expr(&expr(r#""plain""#)), "(str \"plain\")");
    assert_eq!(
        sexp_expr(&expr(r#""a $b c""#)),
        "(interp lit \"a \" (var $b) lit \" c\")"
    );
    assert_eq!(
        sexp_expr(&expr(r#""$a[0] $a[key] $a[-1] $a[$i]""#)),
        "(interp (index (var $a) (int 0)) lit \" \" (index (var $a) (str \"key\")) lit \" \" (index (var $a) (int -1)) lit \" \" (index (var $a) (var $i)))"
    );
    assert_eq!(
        sexp_expr(&expr(r#""$a->b $a?->c""#)),
        "(interp (prop (var $a) b) lit \" \" (prop? (var $a) c))"
    );
    assert_eq!(
        sexp_expr(&expr(r#""x{$a->b[1]}y{$c}z""#)),
        "(interp lit \"x\" (index (prop (var $a) b) (int 1)) lit \"y\" (var $c) lit \"z\")"
    );
    // Escapes inside interpolated strings.
    assert_eq!(
        sexp_expr(&expr(r#""a\nb $c \$d""#)),
        "(interp lit \"a\\nb \" (var $c) lit \" $d\")"
    );
}

#[test]
fn heredocs_and_nowdocs() {
    assert_eq!(sexp_expr(&expr("<<<EOT\nhello\nEOT")), "(str \"hello\")");
    assert_eq!(sexp_expr(&expr("<<<EOT\nEOT")), "(str \"\")");
    assert_eq!(
        sexp_expr(&expr("<<<EOT\na $b c\nEOT")),
        "(interp lit \"a \" (var $b) lit \" c\")"
    );
    // Flexible heredoc: the closing label's indentation is removed from every line.
    assert_eq!(
        sexp_expr(&expr("<<<EOT\n  a\n    b\n  EOT")),
        "(str \"a\\n  b\")"
    );
    // Nowdoc: raw, but indentation is still removed.
    assert_eq!(
        sexp_expr(&expr("<<<'EOT'\n  raw \\n $x\n  EOT")),
        "(str \"raw \\\\n $x\")"
    );
    // Heredoc escapes work like double quotes, except \" stays two characters.
    let e = expr("<<<EOT\na\\\"b\\nc\nEOT");
    let ExpressionKind::Literal(Literal::String(bytes)) = &e.kind else {
        panic!()
    };
    assert_eq!(bytes.as_slice(), b"a\\\"b\nc".as_slice());
    // Quoted label.
    assert_eq!(sexp_expr(&expr("<<<\"EOT\"\n  x\n  EOT")), "(str \"x\")");
}

// --- Precedence and associativity -------------------------------------------

/// Every level of PHP's precedence table, lowest first, with the shape
/// `a OP b OP c` should have.
#[test]
fn precedence_levels() {
    // `or`, `xor`, `and` (left)
    assert_eq!(
        sexp_expr(&expr("$a or $b or $c")),
        "(Or (Or (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a xor $b xor $c")),
        "(Xor (Xor (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a and $b and $c")),
        "(And (And (var $a) (var $b)) (var $c))"
    );
    // assignment (right)
    assert_eq!(
        sexp_expr(&expr("$a = $b = $c")),
        "(= (var $a) (= (var $b) (var $c)))"
    );
    // ternary (left): a ? b : c ? d : e is (a ? b : c) ? d : e
    assert_eq!(
        sexp_expr(&expr("$a ? $b : $c ? $d : $e")),
        "(ternary (ternary (var $a) (var $b) (var $c)) (var $d) (var $e))"
    );
    // `??` (right)
    assert_eq!(
        sexp_expr(&expr("$a ?? $b ?? $c")),
        "(NullCoalesce (var $a) (NullCoalesce (var $b) (var $c)))"
    );
    // `||`, `&&`, `|`, `^`, `&` (left)
    assert_eq!(
        sexp_expr(&expr("$a || $b || $c")),
        "(Or (Or (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a && $b && $c")),
        "(And (And (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a | $b | $c")),
        "(BitwiseOr (BitwiseOr (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a ^ $b ^ $c")),
        "(BitwiseXor (BitwiseXor (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a & $b & $c")),
        "(BitwiseAnd (BitwiseAnd (var $a) (var $b)) (var $c))"
    );
    // `|>` pipe (left)
    assert_eq!(
        sexp_expr(&expr("$a |> $b |> $c")),
        "(Pipe (Pipe (var $a) (var $b)) (var $c))"
    );
    // `.` concat, shifts, `+ -`, `* / %` (left)
    assert_eq!(
        sexp_expr(&expr("$a . $b . $c")),
        "(Concat (Concat (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a << $b >> $c")),
        "(ShiftRight (ShiftLeft (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a + $b - $c")),
        "(Subtract (Add (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a * $b / $c % $d")),
        "(Modulo (Divide (Multiply (var $a) (var $b)) (var $c)) (var $d))"
    );
    // `**` (right)
    assert_eq!(
        sexp_expr(&expr("$a ** $b ** $c")),
        "(Power (var $a) (Power (var $b) (var $c)))"
    );
}

#[test]
fn comparison_levels_and_nonassociativity() {
    // Same-level comparisons are a syntax error.
    assert!(parse("<?php $a == $b == $c;").is_err());
    assert!(parse("<?php $a < $b < $c;").is_err());
    // Different levels mix by level: `==` is lower than `<`.
    assert_eq!(
        sexp_expr(&expr("$a == $b < $c")),
        "(Equal (var $a) (Less (var $b) (var $c)))"
    );
    assert_eq!(
        sexp_expr(&expr("$a < $b == $c")),
        "(Equal (Less (var $a) (var $b)) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a <=> $b")),
        "(Spaceship (var $a) (var $b))"
    );
    assert_eq!(
        sexp_expr(&expr("$a === $b")),
        "(Identical (var $a) (var $b))"
    );
    assert_eq!(
        sexp_expr(&expr("$a !== $b")),
        "(NotIdentical (var $a) (var $b))"
    );
    assert_eq!(sexp_expr(&expr("$a != $b")), "(NotEqual (var $a) (var $b))");
    // instanceof sits between `!` and the casts.
    assert_eq!(
        sexp_expr(&expr("!$a instanceof $b")),
        "(Not (instanceof (var $a) (var $b)))"
    );
    // pipe binds tighter than comparisons, looser than `.`
    assert_eq!(
        sexp_expr(&expr("$a < $b |> $c")),
        "(Less (var $a) (Pipe (var $b) (var $c)))"
    );
    assert_eq!(
        sexp_expr(&expr("$a |> $b . $c")),
        "(Pipe (var $a) (Concat (var $b) (var $c)))"
    );
}

#[test]
fn unary_and_prefix_precedence() {
    assert_eq!(
        sexp_expr(&expr("-$a ** 2")),
        "(Minus (Power (var $a) (int 2)))"
    );
    assert_eq!(
        sexp_expr(&expr("-$a + $b")),
        "(Add (Minus (var $a)) (var $b))"
    );
    assert_eq!(
        sexp_expr(&expr("!$a == $b")),
        "(Equal (Not (var $a)) (var $b))"
    );
    assert_eq!(
        sexp_expr(&expr("~$a + 1")),
        "(Add (BitwiseNot (var $a)) (int 1))"
    );
    assert_eq!(
        sexp_expr(&expr("@$a[0]")),
        "(ErrorSuppress (index (var $a) (int 0)))"
    );
    assert_eq!(
        sexp_expr(&expr("(int) $a + 1")),
        "(Add (cast Int (var $a)) (int 1))"
    );
    assert_eq!(sexp_expr(&expr("(int) -$a")), "(cast Int (Minus (var $a)))");
    assert_eq!(sexp_expr(&expr("- -$a")), "(Minus (Minus (var $a)))");
    // print is very low: it swallows the assignment.
    assert_eq!(
        sexp_expr(&expr("print $a = 1")),
        "(print (= (var $a) (int 1)))"
    );
    // include is lower still.
    assert_eq!(
        sexp_expr(&expr("include $a . $b")),
        "(include Include (Concat (var $a) (var $b)))"
    );
    // clone binds tighter than `**`.
    assert_eq!(
        sexp_expr(&expr("clone $a ** 2")),
        "(Power (clone (var $a) with=none) (int 2))"
    );
    // throw is the lowest: it swallows everything after it.
    assert_eq!(
        sexp_expr(&expr("throw $a . $b")),
        "(throw (Concat (var $a) (var $b)))"
    );
    // yield's key stops before `=>` but includes higher-precedence ops.
    assert_eq!(
        sexp_expr(&expr("yield $a + 1 => $b")),
        "(yield key=(Add (var $a) (int 1)) value=(var $b))"
    );
    assert_eq!(
        sexp_expr(&expr("yield $a or $b")),
        "(Or (yield key=(var $a) value=none) (var $b))"
    );
}

#[test]
fn ternary_forms() {
    assert_eq!(
        sexp_expr(&expr("$a ? $b : $c")),
        "(ternary (var $a) (var $b) (var $c))"
    );
    assert_eq!(
        sexp_expr(&expr("$a ?: $c")),
        "(ternary (var $a) none (var $c))"
    );
    // The then-branch may itself be a ternary.
    assert_eq!(
        sexp_expr(&expr("$a ? $b ? $c : $d : $e")),
        "(ternary (var $a) (ternary (var $b) (var $c) (var $d)) (var $e))"
    );
}

// --- Statement tests ---------------------------------------------------------

#[test]
fn simple_statements() {
    assert_eq!(sexp_stmt(&stmt("echo $a, $b;")), "(echo (var $a) (var $b))");
    assert_eq!(sexp_stmt(&stmt("return;")), "(return none)");
    assert_eq!(sexp_stmt(&stmt("return 1;")), "(return (int 1))");
    assert_eq!(sexp_stmt(&stmt("break;")), "(break 1)");
    assert_eq!(sexp_stmt(&stmt("break 2;")), "(break 2)");
    assert_eq!(sexp_stmt(&stmt("continue;")), "(continue 1)");
    assert_eq!(
        sexp_stmt(&stmt("global $a, $$b;")),
        "(global $a (dyn (var $b)))"
    );
    assert_eq!(
        sexp_stmt(&stmt("static $a = 1, $b;")),
        "(static (a (int 1)) (b none))"
    );
    assert_eq!(
        sexp_stmt(&stmt("unset($a, $b[0]);")),
        "(unset (var $a) (index (var $b) (int 0)))"
    );
    assert_eq!(sexp_stmt(&stmt("goto done;")), "(goto done)");
    assert_eq!(
        stmts("done: echo 1;")
            .iter()
            .map(sexp_stmt)
            .collect::<Vec<_>>(),
        vec!["(label done)".to_string(), "(echo (int 1))".to_string()]
    );
    assert_eq!(sexp_stmt(&stmt("{ echo 1; }")), "(block (echo (int 1)))");
}

#[test]
fn control_flow() {
    assert_eq!(
        sexp_stmt(&stmt("if ($a) echo 1;")),
        "(if (var $a) ((echo (int 1))) none)"
    );
    // A `{ ... }` body is its statements, not a nested block.
    assert_eq!(
        sexp_stmt(&stmt("if ($a) { echo 1; echo 2; }")),
        "(if (var $a) ((echo (int 1)) (echo (int 2))) none)"
    );
    assert_eq!(
        sexp_stmt(&stmt("if ($a) echo 1; elseif ($b) echo 2; else echo 3;")),
        "(if (var $a) ((echo (int 1))) ((if (var $b) ((echo (int 2))) ((echo (int 3))))))"
    );
    // The alternative syntax produces the same nodes.
    assert_eq!(
        sexp_stmt(&stmt(
            "if ($a): echo 1; elseif ($b): echo 2; else: echo 3; endif;"
        )),
        "(if (var $a) ((echo (int 1))) ((if (var $b) ((echo (int 2))) ((echo (int 3))))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("while ($a) { $b++; }")),
        "(while (var $a) ((expr (incdec (var $b) inc=true prefix=false))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("while ($a): echo 1; endwhile;")),
        "(while (var $a) ((echo (int 1))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("do { $x++; } while ($x < 10);")),
        "(do ((expr (incdec (var $x) inc=true prefix=false))) while (Less (var $x) (int 10)))"
    );
    assert_eq!(
        sexp_stmt(&stmt("for ($i = 0; $i < 10; $i++) echo $i;")),
        "(for [(= (var $i) (int 0))] [(Less (var $i) (int 10))] [(incdec (var $i) inc=true prefix=false)] ((echo (var $i))))"
    );
    assert_eq!(sexp_stmt(&stmt("for (;;) {}")), "(for [] [] [] ())");
    assert_eq!(sexp_stmt(&stmt("for (;;): endfor;")), "(for [] [] [] ())");
    assert_eq!(
        sexp_stmt(&stmt("foreach ($a as $k => $v) echo $v;")),
        "(foreach (var $a) key=(var $k) value=(var $v) ref=false ((echo (var $v))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("foreach ($a as &$v) {}")),
        "(foreach (var $a) key=none value=(var $v) ref=true ())"
    );
    assert_eq!(
        sexp_stmt(&stmt("foreach ($a as [$x, $y]) {}")),
        "(foreach (var $a) key=none value=(list (item key=none ref=false spread=false (var $x)) (item key=none ref=false spread=false (var $y))) ref=false ())"
    );
    assert_eq!(
        sexp_stmt(&stmt("foreach ($a as list('k' => $v)): endforeach;")),
        "(foreach (var $a) key=none value=(list (item key=(str \"k\") ref=false spread=false (var $v))) ref=false ())"
    );
}

#[test]
fn switch_statement() {
    assert_eq!(
        sexp_stmt(&stmt(
            "switch ($x) { case 1: echo 1; break; default: echo 2; }"
        )),
        "(switch (var $x) (case (int 1) ((echo (int 1)) (break 1))) (case default ((echo (int 2)))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("switch ($x): case 1; echo 1; endswitch;")),
        "(switch (var $x) (case (int 1) ((echo (int 1)))))"
    );
    assert_eq!(sexp_stmt(&stmt("switch ($x) { }")), "(switch (var $x) )");
}

#[test]
fn try_catch_finally() {
    assert_eq!(
        sexp_stmt(&stmt(
            "try { f(); } catch (A | B $e) { g(); } catch (C) { h(); } finally { i(); }"
        )),
        "(try ((expr (call (name Unqualified:f) ))) [(catch Unqualified:A|Unqualified:B var=e ((expr (call (name Unqualified:g) )))) (catch Unqualified:C var=none ((expr (call (name Unqualified:h) ))))] ((expr (call (name Unqualified:i) ))))"
    );
}

#[test]
fn declare_statement() {
    assert_eq!(
        sexp_stmt(&stmt("declare(strict_types=1);")),
        "(declare (strict_types=(int 1)) none)"
    );
    assert_eq!(
        sexp_stmt(&stmt("declare(strict_types=1) { echo 1; }")),
        "(declare (strict_types=(int 1)) ((echo (int 1))))"
    );
    assert_eq!(
        sexp_stmt(&stmt("declare(ticks=1): echo 1; enddeclare;")),
        "(declare (ticks=(int 1)) ((echo (int 1))))"
    );
}

#[test]
fn namespace_and_use() {
    let program = file("<?php namespace Foo\\Bar; echo 1; namespace Baz { echo 2; }");
    assert_eq!(
        program.statements.iter().map(sexp_stmt).collect::<Vec<_>>(),
        vec![
            "(namespace Qualified:Foo\\Bar braced=false ((echo (int 1))))".to_string(),
            "(namespace Unqualified:Baz braced=true ((echo (int 2))))".to_string(),
        ]
    );
    assert_eq!(
        sexp_stmt(&stmt("namespace { echo 1; }")),
        "(namespace none braced=true ((echo (int 1))))"
    );
    assert_eq!(
        sexp_stmt(&stmt(r"use Foo\Bar as B, Foo\Baz;")),
        "(use (Class Qualified:Foo\\Bar alias=B) (Class Qualified:Foo\\Baz alias=none))"
    );
    assert_eq!(
        stmts(r"use function Foo\bar; use const Foo\X;")
            .iter()
            .map(sexp_stmt)
            .collect::<Vec<_>>(),
        vec![
            "(use (Function Qualified:Foo\\bar alias=none))".to_string(),
            "(use (Constant Qualified:Foo\\X alias=none))".to_string(),
        ]
    );
    assert_eq!(
        sexp_stmt(&stmt(r"use Foo\{Bar, function baz, const X as Y,};")),
        "(use (Class Qualified:Foo\\Bar alias=none) (Function Qualified:Foo\\baz alias=none) (Constant Qualified:Foo\\X alias=Y))"
    );
    assert_eq!(
        sexp_stmt(&stmt(r"use \Foo\Bar;")),
        "(use (Class FullyQualified:Foo\\Bar alias=none))"
    );
}

#[test]
fn const_and_function_declarations() {
    assert_eq!(
        sexp_stmt(&stmt("const X = 1, Y = 'a';")),
        "(const attrs=0 (X = (int 1)) (Y = (str \"a\")))"
    );
    assert_eq!(
        sexp_stmt(&stmt("#[A] const X = 1;")),
        "(const attrs=1 (X = (int 1)))"
    );
    let f = stmt("function &f(int $a, string ...$rest): ?int { return $a; }");
    let sexp = sexp_stmt(&f);
    assert!(sexp.contains("ref=true f"), "{sexp}");
    assert!(sexp.contains("ty=Named:Unqualified:int"), "{sexp}");
    assert!(sexp.contains("variadic=true $rest"), "{sexp}");
    assert!(sexp.contains(": (? Named:Unqualified:int)"), "{sexp}");
}

#[test]
fn class_declarations() {
    let class = |src: &str| sexp_stmt(&stmt(src));
    assert!(class("class A extends B implements C, D {}").contains("kind=Class"));
    assert!(
        class("class A extends B implements C, D {}")
            .contains("extends=[Unqualified:B] implements=[Unqualified:C Unqualified:D]")
    );
    assert!(class("abstract final class A {}").contains("is_abstract: true, is_final: true"));
    assert!(class("readonly class A {}").contains("is_readonly: true"));
    assert!(class("interface I extends A, B {}").contains("kind=Interface"));
    assert!(class("interface I extends A, B {}").contains("extends=[Unqualified:A Unqualified:B]"));
    assert!(class("trait T {}").contains("kind=Trait"));
    assert!(class("enum E: string implements I { case A = 'a'; case B; }").contains("kind=Enum"));
    assert!(
        class("enum E: string implements I { case A = 'a'; case B; }")
            .contains("backing=Named:Unqualified:string")
    );
    assert!(class("enum E {}").contains("backing=none"));
}

#[test]
fn class_members() {
    let members = |src: &str| sexp_stmt(&stmt(src));
    assert!(
        members("class A { public int $x = 1, $y; var $z; }")
            .contains("ty=Named:Unqualified:int [($x = (int 1) hooks=[]) ($y hooks=[])]")
    );
    assert!(members("class A { var $z; }").contains("visibility: Some(Public)"));
    assert!(
        members("class A { public private(set) static ?int $x; }")
            .contains("set_visibility: Some(Private), is_static: true")
    );
    assert!(
        members("class A { const int X = 1, Y = 2; public const Z = 3; }")
            .contains("ty=Named:Unqualified:int [(X = (int 1)) (Y = (int 2))]")
    );
    assert!(members("class A { const LIST = 1; }").contains("(LIST = (int 1))"));
    let m = members(
        "class A { public function list(int $x): void {} abstract protected static function f(); }",
    );
    assert!(m.contains("ref=false list ("), "{m}");
    assert!(m.contains("body=none"), "{m}");
}

#[test]
fn constructor_promotion_and_hooks() {
    let m = sexp_stmt(&stmt(
        "class A { public function __construct(public readonly int $x, private(set) string $y = 'a') {} }",
    ));
    assert!(m.contains("visibility: Some(Public), set_visibility: None, is_static: false, is_abstract: false, is_final: false, is_readonly: true } ty=Named:Unqualified:int"), "{m}");
    assert!(
        m.contains("visibility: None, set_visibility: Some(Private)"),
        "{m}"
    );
    assert!(m.contains("default=(str \"a\")"), "{m}");
    let m = sexp_stmt(&stmt(
        "class A { public string $a { get => $this->a; set(string $v) { $this->a = $v; } } }",
    ));
    assert!(
        m.contains("(hook attrs=0 final=false ref=false get () => (prop (var $this) a))"),
        "{m}"
    );
    assert!(m.contains("(hook attrs=0 final=false ref=false set ((param attrs=0 mods=Modifiers { visibility: None, set_visibility: None, is_static: false, is_abstract: false, is_final: false, is_readonly: false } ty=Named:Unqualified:string ref=false variadic=false $v default=none hooks=0)) ((expr (= (prop (var $this) a) (var $v)))))"), "{m}");
    // `get;` in an interface.
    let m = sexp_stmt(&stmt("interface I { public string $a { get; } }"));
    assert!(
        m.contains("(hook attrs=0 final=false ref=false get () none)"),
        "{m}"
    );
    // Hooks on a promoted parameter.
    let m = sexp_stmt(&stmt(
        "class A { function __construct(public int $x { final &get => $this->x; }) {} }",
    ));
    assert!(m.contains("hooks=1"), "{m}");
    // check the hook itself
    let StatementKind::Class(c) =
        &stmt("class A { function __construct(public int $x { final &get => $this->x; }) {} }")
            .kind
    else {
        panic!()
    };
    let ClassMemberKind::Method(method) = &c.members[0].kind else {
        panic!()
    };
    assert_eq!(method.parameters[0].hooks.len(), 1);
    let hook = &method.parameters[0].hooks[0];
    assert!(hook.is_final && hook.by_reference && hook.name.name == "get");
    assert!(matches!(hook.body, HookBody::Expression(_)));
}

#[test]
fn trait_use_adaptations() {
    let m = sexp_stmt(&stmt(
        "class A { use T1, T2 { T1::foo insteadof T2; T2::bar as baz; foo as private; \\T3::qux as public q; } }",
    ));
    assert!(
        m.contains("(prec Unqualified:T1::foo insteadof [Unqualified:T2])"),
        "{m}"
    );
    assert!(
        m.contains("(alias Unqualified:T2::bar as=baz vis=None)"),
        "{m}"
    );
    assert!(
        m.contains("(alias none::foo as=none vis=Some(Private))"),
        "{m}"
    );
    assert!(
        m.contains("(alias FullyQualified:T3::qux as=q vis=Some(Public))"),
        "{m}"
    );
    assert!(sexp_stmt(&stmt("trait T { use U; }")).contains("(traituse [Unqualified:U] [])"));
}

#[test]
fn types() {
    let ty = |src: &str| {
        let statement = stmt(&format!("function f({src} $x) {{}}"));
        match statement.kind {
            StatementKind::Function(f) => sexp_type(f.parameters[0].ty.as_ref().unwrap()),
            _ => panic!(),
        }
    };
    assert_eq!(ty("int"), "Named:Unqualified:int");
    assert_eq!(ty("?int"), "(? Named:Unqualified:int)");
    assert_eq!(
        ty("int|string"),
        "(union Named:Unqualified:int Named:Unqualified:string)"
    );
    assert_eq!(ty("A&B"), "(inter Named:Unqualified:A Named:Unqualified:B)");
    assert_eq!(
        ty("A&B&C"),
        "(inter Named:Unqualified:A Named:Unqualified:B Named:Unqualified:C)"
    );
    assert_eq!(
        ty("(A&B)|C"),
        "(union (inter Named:Unqualified:A Named:Unqualified:B) Named:Unqualified:C)"
    );
    assert_eq!(ty("array"), "Named:Unqualified:array");
    assert_eq!(ty("callable"), "Named:Unqualified:callable");
    assert_eq!(ty("\\Foo\\Bar"), "Named:FullyQualified:Foo\\Bar");
    assert_eq!(ty("Foo\\Bar"), "Named:Qualified:Foo\\Bar");
    assert_eq!(ty("null"), "Named:Unqualified:null");
    assert_eq!(ty("true"), "Named:Unqualified:true");
    // `static` parses as a promotion modifier on a parameter (PHP rejects it
    // at compile time), and is a valid return type.
    assert!(parse("<?php function f(static $x) {}").is_ok());
    let statement = stmt("function f(): static {}");
    match statement.kind {
        StatementKind::Function(f) => assert_eq!(
            sexp_type(f.return_type.as_ref().unwrap()),
            "Named:Unqualified:static"
        ),
        _ => panic!(),
    }
    // `?A|B` is not valid PHP.
    assert!(parse("<?php function f(?A|B $x) {}").is_err());
}

#[test]
fn attributes_are_flattened() {
    let s = sexp_stmt(&stmt("#[A, B(1), C(name: 'x')] class D {}"));
    assert!(s.contains("attrs=3"), "{s}");
}

#[test]
fn files_tags_and_inline_html() {
    let program = file("<h1>Hi</h1><?= $a ?>\n<?php echo 1; ?>\ntrailing");
    let kinds: Vec<String> = program.statements.iter().map(sexp_stmt).collect();
    assert_eq!(
        kinds,
        vec![
            "(html \"<h1>Hi</h1>\")".to_string(),
            "(echo (var $a))".to_string(),
            "(echo (int 1))".to_string(),
            "(html \"trailing\")".to_string(),
        ]
    );
    // A close tag ends a statement like `;`.
    let program = file("<?php echo 1 ?>");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn halt_compiler() {
    let prefix = "<?php __halt_compiler();";
    let program = file("<?php __halt_compiler(); raw \x00 data");
    assert_eq!(
        sexp_stmt(&program.statements[0]),
        format!("(halt {})", prefix.len())
    );
    let program = file("<?php __halt_compiler();");
    assert_eq!(
        sexp_stmt(&program.statements[0]),
        format!("(halt {})", prefix.len())
    );
}

#[test]
fn static_ambiguity() {
    // `static $x = 1;` is a static variable...
    assert_eq!(sexp_stmt(&stmt("static $x = 1;")), "(static (x (int 1)))");
    // ... `static function () {}` is a static closure expression.
    assert_eq!(
        sexp_stmt(&stmt("static function () {};")),
        "(expr (closure (attrs=0 static=true ref=false () uses=[] : none ())))"
    );
    assert_eq!(
        sexp_stmt(&stmt("$f = static fn() => 1;")),
        "(expr (= (var $f) (arrow (attrs=0 static=true ref=false () : none => (int 1)))))"
    );
    // `static::foo()` is a class reference.
    assert_eq!(
        sexp_expr(&expr("static::foo()")),
        "(staticcall (name Unqualified:static) foo )"
    );
    assert_eq!(
        sexp_expr(&expr("static::$x")),
        "(staticprop (name Unqualified:static) $x)"
    );
}

// --- Error tests -------------------------------------------------------------

#[test]
fn errors_have_sensible_spans() {
    let error = err("$a = ;");
    assert!(error.message.contains("unexpected"), "{}", error.message);
    assert_eq!(error.span.start, "<?php\n$a = ".len());

    let error = err("1 = 2;");
    assert!(error.message.contains("assign"), "{}", error.message);

    let error = err("foo(1");
    assert!(error.message.contains("')'"), "{}", error.message);

    let error = err("class { }");
    assert!(error.message.contains("identifier"), "{}", error.message);

    // Unterminated string is a lexer error, surfaced with its span.
    let error = err(r#""abc"#);
    assert_eq!(error.span.start, "<?php\n".len());

    // Unsupported syntax (backticks) propagates as a ParseError.
    let error = err("`ls`;");
    assert!(error.is_unsupported_lex(), "{}", error.message);

    // Invalid octal.
    assert!(parse("<?php 08;").is_err());
    assert!(parse("<?php 0_8;").is_err());
    assert!(parse("<?php 0777;").is_ok());

    // Non-assignable targets.
    assert!(parse("<?php 1 = 2;").is_err());
    assert!(parse("<?php FOO = 1;").is_err());
    assert!(parse("<?php $a = &1;").is_err());
    assert!(parse("<?php [$a] += 1;").is_err());

    // `__halt_compiler` inside a block parses (PHP raises a CompileError,
    // which is past the grammar) and closes every open block.
    assert!(parse("<?php function f() { __halt_compiler(); }").is_ok());
}

#[test]
fn deep_nesting_is_bounded() {
    let mut src = String::from("<?php ");
    for _ in 0..30_000 {
        src.push('(');
    }
    src.push('1');
    for _ in 0..30_000 {
        src.push(')');
    }
    src.push(';');
    let error = parse(&src).expect_err("deep nesting must be an error, not a crash");
    assert!(error.message.contains("too deep"), "{}", error.message);
}
