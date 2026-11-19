//! Class-local checks use declarations and direct references such as `$this->run()`.
//! Computed names and trait consumers can refer to private members indirectly.

use super::{
    Context,
    rule::{DefaultStatus, Group, Rule, Visit},
    traversal::{Node, Scope},
};
use phera_ast::{
    ClassDeclaration, ClassKind, ClassMemberKind as Member, ExpressionKind as Expr, MemberName,
    Method, UseKind, Variable, Visibility,
};
use phera_source::Span;
use std::collections::{HashMap, HashSet};

pub(super) const RULES: &[Rule] = &[
    rule(
        Group::Unused,
        "unused-private-method",
        "Detect a private method without a class-local reference.",
        "class C { private function f() {} }",
        "class C {}",
        DefaultStatus::OptIn,
    ),
    rule(
        Group::Unused,
        "unused-private-property",
        "Detect a private property without a class-local read.",
        "class C { private int $x; function reset() { $this->x = 0; } }",
        "class C {}",
        DefaultStatus::Enabled,
    ),
    rule(
        Group::Unused,
        "unused-private-constant",
        "Detect a private constant without a class-local reference.",
        "class C { private const X = 1; }",
        "class C {}",
        DefaultStatus::OptIn,
    ),
    rule(
        Group::Suspicious,
        "missing-parent-constructor",
        "Detect a constructor that omits a parent constructor with required parameters.",
        "class P { function __construct($x) {} } class C extends P { function __construct() {} }",
        "class P { function __construct($x) {} } class C extends P { function __construct() { parent::__construct(1); } }",
        DefaultStatus::OptIn,
    ),
    rule(
        Group::Suspicious,
        "duplicate-promoted-property",
        "Detect a promoted property also declared in the class body.",
        "class C { public int $x; function __construct(public int $x) {} }",
        "class C { function __construct(public int $x) {} }",
        DefaultStatus::Enabled,
    ),
    rule(
        Group::Suspicious,
        "readonly-write-outside-constructor",
        "Detect direct writes to readonly properties outside the constructor.",
        "class C { public readonly int $x; public function f() { $this->x = 1; } }",
        "class C { public readonly int $x; public function __construct() { $this->x = 1; } }",
        DefaultStatus::OptIn,
    ),
    rule(
        Group::Suspicious,
        "magic-method-signature",
        "Validate PHP magic method signatures and visibility.",
        "class C { private function __get($key) {} }",
        "class C { public function __get($key) {} }",
        DefaultStatus::Enabled,
    ),
];

const fn rule(
    group: Group,
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
    default: DefaultStatus,
) -> Rule {
    Rule {
        group,
        name,
        description,
        bad,
        good,
        default,
        fix: "",
        visit: Visit::Namespace,
        check: None,
    }
}

#[derive(Default)]
pub(super) struct Members<'a> {
    classes: HashMap<usize, ClassUses<'a>>,
}

#[derive(Default)]
struct ClassUses<'a> {
    names: HashSet<String>,
    property_reads: HashSet<String>,
    direct_writes: HashSet<usize>,
    retained_references: HashSet<&'a str>,
    stored_statics: HashSet<&'a str>,
    cleared_statics: HashSet<&'a str>,
    writes: Vec<(&'a str, Span, Option<usize>)>,
    parent_constructed: bool,
    dynamic: bool,
}

impl<'a> Members<'a> {
    pub(super) fn observe(&mut self, context: &mut Context<'_>, node: Node<'a>, scope: Scope<'a>) {
        if let Node::Method(method) = node
            && context
                .enabled
                .severities
                .contains_key("magic-method-signature")
            && let Some(class) = scope.class
        {
            magic_method(context, class, method);
        }
        if !context.enabled.member_references {
            return;
        }
        let Node::Expression(expression, _) = node else {
            return;
        };
        let Some(class) = scope.class else {
            return;
        };
        let uses = self.classes.entry(class.span.start).or_default();
        match &expression.kind {
            Expr::StaticCall { class, name, .. } => {
                if let Expr::Name(class) = &class.kind
                    && class.text.eq_ignore_ascii_case("parent")
                    && let MemberName::Identifier(name) = name
                    && name.name.eq_ignore_ascii_case("__construct")
                {
                    uses.parent_constructed = true;
                }
                uses.member(name);
            }
            Expr::MethodCall { name, .. } | Expr::ClassConstant { name, .. } => uses.member(name),
            Expr::Property { name, .. } => match name {
                MemberName::Identifier(name) => uses.read_property(&name.name, expression.span),
                MemberName::Expression(_) => uses.dynamic = true,
            },
            Expr::StaticProperty { name, .. } => match name {
                Variable::Named(name) => uses.read_property(name, expression.span),
                Variable::Dynamic(_) => uses.dynamic = true,
            },
            Expr::Literal(phera_ast::Literal::String(name)) => {
                if let Ok(name) = std::str::from_utf8(name) {
                    uses.names.insert(name.to_ascii_lowercase());
                    uses.property_reads.insert(name.to_owned());
                }
            }
            Expr::Assign { target, value } => {
                uses.assignment(target, value);
                uses.property_write(target, scope.callable);
            }
            Expr::AssignReference { target, .. }
            | Expr::CompoundAssign { target, .. }
            | Expr::IncDec { target, .. } => uses.property_write(target, scope.callable),
            Expr::Eval(_) | Expr::Include { .. } => uses.dynamic = true,
            _ => {}
        }
    }

    pub(super) fn report(&self, context: &mut Context<'_>, class: &ClassDeclaration) {
        let empty = ClassUses::default();
        let uses = self.classes.get(&class.span.start).unwrap_or(&empty);
        let can_check_private = !uses.dynamic
            && class.kind == ClassKind::Class
            && !class
                .members
                .iter()
                .any(|member| matches!(member.kind, Member::TraitUse { .. }));
        let mut declared = HashSet::new();
        let mut readonly = HashSet::new();
        let mut constructor = None;
        for member in &class.members {
            match &member.kind {
                Member::Method(method) => {
                    if method.name.name.eq_ignore_ascii_case("__construct") {
                        constructor = Some(method);
                    } else if can_check_private
                        && method.modifiers.visibility == Some(Visibility::Private)
                        && !method.name.name.starts_with("__")
                        && method.attributes.is_empty()
                    {
                        uses.check(
                            context,
                            "unused-private-method",
                            &method.name.name,
                            method.name.span,
                        );
                    }
                }
                Member::Property {
                    modifiers,
                    items,
                    attributes,
                    ..
                } => {
                    for item in items {
                        declared.insert(item.name.as_str());
                        if modifiers.is_readonly || class.modifiers.is_readonly {
                            readonly.insert(item.name.as_str());
                        }
                        if can_check_private
                            && modifiers.visibility == Some(Visibility::Private)
                            && attributes.is_empty()
                        {
                            uses.check_property(context, &item.name, item.span);
                        }
                    }
                }
                Member::Constant {
                    modifiers,
                    items,
                    attributes,
                    ..
                } if can_check_private
                    && modifiers.visibility == Some(Visibility::Private)
                    && attributes.is_empty() =>
                {
                    for item in items {
                        uses.check(
                            context,
                            "unused-private-constant",
                            &item.name.name,
                            item.name.span,
                        );
                    }
                }
                _ => {}
            }
        }
        if let Some(constructor) = constructor {
            for parameter in constructor
                .parameters
                .iter()
                .filter(|param| param.is_promoted())
            {
                if declared.contains(parameter.name.as_str()) {
                    context.report(
                        "duplicate-promoted-property",
                        parameter.span,
                        format!(
                            "Promoted property ${} is also declared in the class body.",
                            parameter.name
                        ),
                    );
                }
                if parameter.modifiers.is_readonly || class.modifiers.is_readonly {
                    readonly.insert(&parameter.name);
                }
            }
            if !uses.parent_constructed && constructor.body.is_some() {
                parent_constructor(context, class, constructor);
            }
        }
        for &(name, span, callable) in &uses.writes {
            if readonly.contains(name) && callable != constructor.map(|method| method.span.start) {
                context.report(
                    "readonly-write-outside-constructor",
                    span,
                    format!("Readonly property ${name} is written outside the constructor."),
                );
            }
        }
    }
}

impl<'a> ClassUses<'a> {
    fn assignment(&mut self, target: &'a phera_ast::Expression, value: &phera_ast::Expression) {
        match &target.kind {
            Expr::Property {
                name: MemberName::Identifier(name),
                ..
            } => {
                self.direct_writes.insert(target.span.start);
                // A property can keep aliases alive across clones through an array of references.
                let mut pending = vec![value];
                while let Some(value) = pending.pop() {
                    if let Expr::Array(items) = &value.kind {
                        if items.iter().any(|item| item.by_reference) {
                            self.retained_references.insert(&name.name);
                            break;
                        }
                        pending.extend(items.iter().map(|item| &item.value));
                    }
                }
            }
            Expr::StaticProperty {
                name: Variable::Named(name),
                ..
            } => {
                self.direct_writes.insert(target.span.start);
                // A static slot that is filled and later cleared can hold a memory reserve or resource.
                if matches!(value.kind, Expr::Literal(phera_ast::Literal::Null)) {
                    self.cleared_statics.insert(name);
                } else {
                    self.stored_statics.insert(name);
                }
            }
            _ => {}
        }
    }

    fn property_write(&mut self, target: &'a phera_ast::Expression, callable: Option<usize>) {
        if let Expr::Property {
            target: receiver,
            name: MemberName::Identifier(name),
            ..
        } = &target.kind
            && matches!(&receiver.kind, Expr::Variable(Variable::Named(name)) if name == "this")
        {
            self.writes.push((&name.name, name.span, callable));
        }
    }

    fn read_property(&mut self, name: &str, span: Span) {
        if !self.direct_writes.contains(&span.start) {
            self.property_reads.insert(name.to_owned());
        }
    }

    fn check_property(&self, context: &mut Context<'_>, name: &str, span: Span) {
        if !self.property_reads.contains(name)
            && !self.retained_references.contains(name)
            && !(self.stored_statics.contains(name) && self.cleared_statics.contains(name))
        {
            context.report(
                "unused-private-property",
                span,
                format!("Private property '${name}' is never read in this class."),
            );
        }
    }

    fn member(&mut self, name: &MemberName) {
        match name {
            MemberName::Identifier(name) => {
                self.names.insert(name.name.to_ascii_lowercase());
            }
            MemberName::Expression(_) => self.dynamic = true,
        }
    }

    fn check(&self, context: &mut Context<'_>, rule: &'static str, name: &str, span: Span) {
        if !self.names.contains(&name.to_ascii_lowercase()) {
            context.report(
                rule,
                span,
                format!("Private member '{name}' is never referenced in this class."),
            );
        }
    }
}

fn parent_constructor(context: &mut Context<'_>, class: &ClassDeclaration, method: &Method) {
    if !context
        .enabled
        .severities
        .contains_key("missing-parent-constructor")
    {
        return;
    }
    let Some(parent) = class.extends.first() else {
        return;
    };
    let names = context.names.resolve(parent, UseKind::Class);
    let Some(name) = names.first() else {
        return;
    };
    if context
        .index
        .and_then(|index| index.method(name, "__construct"))
        .is_some_and(|signature| {
            signature.modifiers.visibility != Some(Visibility::Private)
                && signature
                    .parameters
                    .iter()
                    .any(|arg| !arg.optional && !arg.variadic)
        })
    {
        context.report(
            "missing-parent-constructor",
            method.name.span,
            format!("Parent class '{name}' has a constructor with required arguments."),
        );
    }
}

fn magic_method(context: &mut Context<'_>, class: &ClassDeclaration, method: &Method) {
    let class = class.name.as_ref().map_or_else(
        || "class@anonymous".into(),
        |name| context.names.declaration(&name.name),
    );
    let mut warning = None;
    let outcome = phera_parser::magic_methods::validate(
        &class,
        method,
        |ty| {
            std::str::from_utf8(context.source.slice(ty.span))
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase()
        },
        |warning_message| warning = Some(warning_message),
    );
    if let Some(message) = outcome.err().or(warning) {
        context.report("magic-method-signature", method.name.span, message);
    }
}
