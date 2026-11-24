//! `function size(): int { return 'large'; }` conflicts with its return contract.
//! At level 3 PHPStan checks known mismatches, including documented property types.

use super::{statements::Flow, *};
use crate::types::{Compatibility, TypePolicy};

impl Analyzer<'_> {
    pub fn check_return_value(
        &mut self,
        expression: Option<&Expression>,
        given: &ValueType,
        scope: &Scope,
        span: Span,
    ) {
        if self.level < 3 || self.context.generator || self.context.scope_kind == ScopeKind::Global
        {
            return;
        }
        let expected = self
            .context
            .documented_return
            .as_ref()
            .unwrap_or(&self.context.returns);
        let expected = self
            .context
            .class
            .as_ref()
            .map_or_else(
                || expected.clone(),
                |class| self.relative_type(expected.clone(), class),
            )
            .value_choices();
        let name = &self.context.function;
        let template =
            expression.and_then(|expr| self.mismatched_template_return(expr, &expected, given));
        match (expression, &expected) {
            (_, ValueType::Never) => self.diagnose(
                "return.never",
                span,
                format!("{name}() should never return, but a return statement was found."),
            ),
            (None, _) if allows_empty_return(&expected) => {}
            (None, _) => self.diagnose(
                "return.empty",
                span,
                format!("{name}() should return {expected}, but returned without a value."),
            ),
            (Some(_), ValueType::Void) if self.context.returns != ValueType::Void => {
                self.diagnose(
                    "return.void",
                    span,
                    format!("{name}() has return type void, but returns {given}."),
                );
            }
            (Some(expr), _)
                if expected != ValueType::Void
                    && (template.is_some()
                        || self.incompatible(&expected, given, self.value_policy())) =>
            {
                let provenance = self.narrowed_at(expr, scope);
                let callable = if name == "Closure" {
                    "Anonymous function".to_owned()
                } else if let Some(class) = &self.context.class {
                    format!("Method {class}::{name}()")
                } else {
                    format!("Function {name}()")
                };
                let given = super::argument_contracts::type_for_message(given, &expected);
                let expected = template.unwrap_or_else(|| expected.to_string());
                self.diagnose(
                    "return.type",
                    span,
                    format!("{callable} should return {expected} but returns {given}.{provenance}"),
                );
            }
            _ => {}
        }
    }

    pub(super) fn value_policy(&self) -> TypePolicy {
        if self.strict_types {
            TypePolicy::Strict
        } else {
            TypePolicy::WeakValue
        }
    }

    pub fn incompatible(
        &self,
        expected: &ValueType,
        given: &ValueType,
        policy: TypePolicy,
    ) -> bool {
        let given = given.value_choices();
        let variants = match &given {
            ValueType::Union(types) => types.as_ref(),
            _ => std::slice::from_ref(&given),
        };
        let checked = variants.iter().filter(|ty| {
            self.level >= 8
                || **ty != ValueType::Null
                || variants.len() == 1
                || !expected.excludes_null()
        });
        let rejects = |ty: &ValueType| {
            if self.strict_mixed(ty) && !super::strict_mixed::accepts_mixed(expected) {
                return true;
            }
            if self.level >= 7
                && matches!((expected, ty), (ValueType::ClassString(bound), ValueType::ClassString(actual))
                    if bound.class_name().is_some() && **actual == ValueType::Object)
            {
                return true;
            }
            if self.level >= 7
                && super::integer_range_contracts::range_excludes_values(expected, ty)
            {
                return true;
            }
            expected.accepts(ty, self.index, policy) == Compatibility::Rejects
        };
        if self.level >= 7 {
            checked.into_iter().any(rejects)
        } else {
            checked.into_iter().all(rejects)
        }
    }

    pub fn narrowed_at(&self, expression: &Expression, scope: &Scope) -> String {
        if scope.facts.is_empty() {
            return String::new();
        }
        crate::scope::Place::of(expression)
            .and_then(|place| scope.facts.get(&place)?.narrowed_at)
            .map_or_else(String::new, |span| {
                format!(
                    " The type was narrowed on line {}.",
                    self.file.lines.line(span.start)
                )
            })
    }

    pub fn check_property_assignment(
        &mut self,
        target: &Expression,
        place: Option<&crate::scope::Place>,
        expected: &ValueType,
        given: &ValueType,
        scope: &Scope,
    ) {
        if self.level >= 3 && self.incompatible(expected, given, self.value_policy()) {
            let name = match &target.kind {
                ExpressionKind::Property {
                    name: MemberName::Identifier(name),
                    ..
                } => Some(name.name.as_str()),
                ExpressionKind::StaticProperty {
                    name: Variable::Named(name),
                    ..
                } => Some(name.as_str()),
                _ => None,
            };
            if let Some(name) = name {
                let property = place
                    .and_then(|place| self.resolved_property(place, scope))
                    .map_or_else(
                        || format!("${name}"),
                        |(class, _)| format!("{}::${name}", class.name),
                    );
                let expected = super::member_checks::receiver_name(expected);
                let given = super::member_checks::receiver_name(given);
                self.diagnose(
                    "assign.propertyType",
                    target.span,
                    format!("Property {property} ({expected}) does not accept {given}."),
                );
            }
        }
    }

    pub fn check_property_default(&mut self, name: &str, given: &ValueType, span: Span) {
        if self.level < 3 {
            return;
        }
        let Some(class) = self.context.class.as_deref() else {
            return;
        };
        let Some(property) = self.index.property(class, name) else {
            return;
        };
        let expected = self.relative_type(property.effective_type(self.index), class);
        if self.incompatible(&expected, given, self.value_policy()) {
            self.diagnose("property.defaultValue", span, format!("Property {class}::${name} ({expected}) does not accept a default of type {given}."));
        }
    }

    pub fn check_return_path(&mut self, flow: Flow, body: &[Statement], span: Span) {
        if self.context.returns == ValueType::Void {
            self.void_returns(body);
            return;
        }
        if self.context.incomplete
            || flow != Flow::Next
            || self.context.generator
            || (self.context.returns == ValueType::Never && self.level == 0)
        {
            return;
        }
        let documented = self.fallthrough_doc_type();
        if !self.context.declared_return && documented.is_none() {
            return;
        }
        let mut blocks = vec![body];
        while let Some(block) = blocks.pop() {
            for statement in block {
                // Syntactic fallthrough cannot prove that a goto body is
                // missing a return. Each explicit return is still checked.
                if matches!(statement.kind, StatementKind::Goto(_)) {
                    return;
                }
                blocks.extend(crate::collect::child_blocks(statement));
            }
        }
        let span = body.last().map_or_else(
            || {
                if documented.is_some() {
                    empty_body_location(&self.file.source, span)
                } else {
                    span
                }
            },
            |statement| statement.span,
        );
        if let Some(documented) = documented {
            self.report_doc_fallthrough(&documented, span);
        } else {
            self.report(Finding::MissingReturn(self.context.function.clone()), span);
        }
    }
    fn void_returns(&mut self, body: &[Statement]) {
        let mut pending = vec![body];
        while let Some(block) = pending.pop() {
            for statement in block {
                if matches!(statement.kind, StatementKind::Return(Some(_))) {
                    self.report(Finding::VoidReturn, statement.span);
                }
                pending.extend(crate::collect::child_blocks(statement));
            }
        }
    }
}

pub(super) fn allows_empty_return(ty: &ValueType) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            ty if ty.is_mixed() || *ty == ValueType::Void => return true,
            ValueType::Union(types) => pending.extend(types.iter()),
            ValueType::Template(template) => pending.push(&template.bound),
            ValueType::Conditional(conditional) => {
                pending.extend([&conditional.yes, &conditional.no])
            }
            _ => {}
        }
    }
    false
}

pub(super) fn empty_body_location(source: &str, declaration: Span) -> Span {
    let Some(code) = source.get(declaration.start..declaration.end) else {
        return declaration;
    };
    let Some((body, _)) = code.rsplit_once('}') else {
        return declaration;
    };
    // With a comment-only body, PHPStan points at the comment just before
    // the closing brace rather than at the function signature.
    let end = declaration.start + body.trim_end().len();
    Span::new(end.saturating_sub(1).max(declaration.start), end)
}
