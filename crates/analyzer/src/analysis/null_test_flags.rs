//! In `$ready = $node !== null; if ($ready) $node->run();`, the flag carries
//! the non-null fact while both locals keep their values.

use super::*;
use crate::scope::Place;

impl Analyzer<'_> {
    pub(super) fn remember_null_test_flag(
        &self,
        flag: &str,
        value: &Expression,
        scope: &mut Scope,
    ) {
        if self.level < 8 || scope.dynamic || self.context.scope_kind == ScopeKind::Global {
            return;
        }
        let ExpressionKind::Binary {
            operator: operator @ (BinaryOperator::Identical | BinaryOperator::NotIdentical),
            left,
            right,
        } = &value.kind
        else {
            return;
        };
        let variable = match (&left.kind, &right.kind) {
            (
                ExpressionKind::Literal(Literal::Null),
                ExpressionKind::Variable(Variable::Named(name)),
            )
            | (
                ExpressionKind::Variable(Variable::Named(name)),
                ExpressionKind::Literal(Literal::Null),
            ) => name,
            _ => return,
        };
        if flag != variable
            && [flag, variable].iter().all(|name| {
                !scope.aliases.contains_key(*name) && !scope.external_references.contains(*name)
            })
        {
            scope.facts.remember_null_comparison(
                flag,
                variable,
                *operator == BinaryOperator::NotIdentical,
            );
        }
    }

    pub(super) fn refine_null_test_flag(&self, flag: &str, truth: bool, scope: &mut Scope) {
        if self.level < 8 || scope.facts.is_empty() {
            return;
        }
        let variables: Vec<_> = scope
            .facts
            .non_null_from_flag(flag, truth)
            .map(str::to_owned)
            .collect();
        for variable in variables {
            let original = scope.get(&variable).ty;
            let ty = original.without_null();
            if ty != original && ty != ValueType::Never {
                scope.facts.record(Place::variable(&variable), ty, None);
            }
        }
    }

    pub(super) fn remember_branch_null_tests(
        &self,
        flag: &str,
        branches: &super::condition::Condition,
        scope: &mut Scope,
    ) {
        if self.level < 8
            || scope.dynamic
            || self.context.scope_kind == ScopeKind::Global
            || scope.external_references.contains(flag)
            || scope.aliases.contains_key(flag)
        {
            return;
        }
        for (truth, branch) in [(true, &branches.yes), (false, &branches.no)] {
            let Some(branch) = branch else { continue };
            for name in branch.variables.keys() {
                if name != flag
                    && !scope.external_references.contains(name)
                    && !scope.get(name).ty.excludes_null()
                    && branch.get(name).ty.excludes_null()
                {
                    scope.facts.remember_null_comparison(flag, name, truth);
                }
            }
        }
    }
}
