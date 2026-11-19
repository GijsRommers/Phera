//! Match branches join independently: `match ($flag) { true => throw ..., false => $x = 1 }`.
//! PHP throws for an unmatched subject, so only selected arm bodies can continue.

use super::*;

impl Analyzer<'_> {
    pub fn match_expression(&mut self, expression: &Match, scope: &mut Scope) -> ValueType {
        let subject = crate::constants::value(&expression.subject, scope);
        if self.expression(&expression.subject, scope, false) == ValueType::Never {
            return ValueType::Never;
        }
        let mut remaining = Some(scope.clone());
        let mut paths = Vec::new();
        let mut result = ValueType::Never;
        for arm in &expression.arms {
            let Some(conditions) = &arm.conditions else {
                continue;
            };
            let mut selected = Vec::new();
            for condition in conditions {
                let Some(mut branch) = remaining.take() else {
                    break;
                };
                let value = crate::constants::value(condition, &branch);
                if let Some(crate::constants::ConstantValue::Bool(subject)) = subject
                    && value.is_none()
                {
                    let branches = self.condition(condition, branch, false);
                    let (hits, misses) = if subject {
                        (branches.yes, branches.no)
                    } else {
                        (branches.no, branches.yes)
                    };
                    // A truthy non-boolean value can still fail strict matching.
                    remaining = if branches.ty == ValueType::Bool {
                        misses
                    } else {
                        condition::Condition::merge(hits.clone(), misses)
                    };
                    selected.extend(hits);
                    continue;
                }
                if self.expression(condition, &mut branch, false) == ValueType::Never {
                    break;
                }
                let matches = subject.as_ref().zip(value.as_ref()).map(|(a, b)| a == b);
                if matches != Some(false) {
                    let mut selected_scope = branch.clone();
                    if self.level >= 2 {
                        self.match_class_arm(&expression.subject, condition, &mut selected_scope);
                    }
                    selected.push(selected_scope);
                }
                if matches != Some(true) {
                    remaining = Some(branch);
                }
            }
            if !selected.is_empty() {
                self.match_arm(&arm.body, Scope::join(&selected), &mut paths, &mut result);
            }
        }
        if let Some(remaining) = remaining {
            if let Some(default) = expression.arms.iter().find(|a| a.conditions.is_none()) {
                self.match_arm(&default.body, remaining, &mut paths, &mut result);
            } else {
                self.record_exception(&remaining);
            }
        }
        if !paths.is_empty() {
            *scope = Scope::join(&paths);
        }
        result
    }

    fn match_class_arm(&mut self, subject: &Expression, condition: &Expression, scope: &mut Scope) {
        let (
            ExpressionKind::ClassConstant {
                class: variable,
                name: MemberName::Identifier(subject_member),
            },
            ExpressionKind::ClassConstant {
                class,
                name: MemberName::Identifier(case_member),
            },
        ) = (&subject.kind, &condition.kind)
        else {
            return;
        };
        // `match ($node::class) { EnumNode::class => $node->choices() }`
        // is a strict class match even when $node has an interface type.
        if subject_member.name.eq_ignore_ascii_case("class")
            && case_member.name.eq_ignore_ascii_case("class")
            && let ExpressionKind::Name(name) = &class.kind
            && let Some(class) = self.class_name(name, false)
        {
            self.refine_type(
                variable,
                ValueType::Class(class),
                scope,
                true,
                condition.span,
            );
        }
    }

    fn match_arm(
        &mut self,
        expression: &Expression,
        mut scope: Scope,
        paths: &mut Vec<Scope>,
        result: &mut ValueType,
    ) {
        let ty = self.expression(expression, &mut scope, false);
        *result = result.join(&ty);
        if ty != ValueType::Never {
            paths.push(scope);
        }
    }
}
