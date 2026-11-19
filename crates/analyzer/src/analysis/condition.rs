//! Boolean outcomes keep their own scopes: `$flag && ($x = read()) && use_it($x)`.
//! The last operand is reached only through the left side's true outcome.

use super::*;

pub struct Condition {
    pub yes: Option<Scope>,
    pub no: Option<Scope>,
    pub ty: ValueType,
}

impl Default for Condition {
    fn default() -> Self {
        Self {
            yes: None,
            no: None,
            ty: ValueType::Never,
        }
    }
}

impl Condition {
    fn boolean(&mut self) {
        if self.yes.is_some() || self.no.is_some() {
            self.ty = ValueType::Bool;
        }
    }

    pub fn merge(yes: Option<Scope>, no: Option<Scope>) -> Option<Scope> {
        match (yes, no) {
            (Some(yes), Some(no)) => Some(Scope::join(&[yes, no])),
            (yes, no) => yes.or(no),
        }
    }

    pub fn after(self) -> Option<Scope> {
        Self::merge(self.yes, self.no)
    }
}

#[derive(Clone, Copy)]
enum Gate {
    And,
    Or,
}

enum Step<'a> {
    Evaluate(&'a Expression, Scope),
    Negate,
    Right(Gate, &'a Expression),
    Combine(Gate, Option<Scope>),
    Ternary(Option<&'a Expression>, &'a Expression),
    Alternative(&'a Expression, Option<Scope>),
    Join(Condition),
}

impl Analyzer<'_> {
    pub fn condition(&mut self, expression: &Expression, scope: Scope, quiet: bool) -> Condition {
        let mut pending = vec![Step::Evaluate(expression, scope)];
        let mut outcome = Condition::default();
        while let Some(step) = pending.pop() {
            match step {
                Step::Evaluate(expression, scope) => match &expression.kind {
                    ExpressionKind::Binary {
                        operator,
                        left,
                        right,
                    } if matches!(operator, BinaryOperator::And | BinaryOperator::Or) => {
                        let gate = if *operator == BinaryOperator::And {
                            Gate::And
                        } else {
                            Gate::Or
                        };
                        pending.push(Step::Right(gate, right));
                        pending.push(Step::Evaluate(left, scope));
                    }
                    ExpressionKind::Binary {
                        operator: BinaryOperator::NullCoalesce,
                        left,
                        right,
                    } => {
                        let (non_null, fallback) = self.coalescing_condition(left, scope);
                        outcome = non_null;
                        if let Some(scope) = fallback {
                            pending.push(Step::Join(std::mem::take(&mut outcome)));
                            pending.push(Step::Evaluate(right, scope));
                        }
                    }
                    ExpressionKind::Unary {
                        operator: UnaryOperator::Not,
                        operand,
                    } => {
                        pending.push(Step::Negate);
                        pending.push(Step::Evaluate(operand, scope));
                    }
                    ExpressionKind::Ternary {
                        condition,
                        then,
                        otherwise,
                    } => {
                        pending.push(Step::Ternary(then.as_deref(), otherwise));
                        pending.push(Step::Evaluate(condition, scope));
                    }
                    _ => outcome = self.condition_leaf(expression, scope, quiet),
                },
                Step::Negate => {
                    std::mem::swap(&mut outcome.yes, &mut outcome.no);
                    outcome.boolean();
                }
                Step::Right(gate, right) => {
                    let continuing = match gate {
                        Gate::And => outcome.yes.take(),
                        Gate::Or => outcome.no.take(),
                    };
                    if let Some(scope) = continuing {
                        let shortcut = match gate {
                            Gate::And => outcome.no.take(),
                            Gate::Or => outcome.yes.take(),
                        };
                        pending.push(Step::Combine(gate, shortcut));
                        pending.push(Step::Evaluate(right, scope));
                    } else {
                        outcome.boolean();
                    }
                }
                Step::Combine(Gate::And, skipped) => {
                    outcome.no = Condition::merge(skipped, outcome.no.take());
                    outcome.boolean();
                }
                Step::Combine(Gate::Or, skipped) => {
                    outcome.yes = Condition::merge(skipped, outcome.yes.take());
                    outcome.boolean();
                }
                Step::Ternary(then, otherwise) => {
                    let no = outcome.no.take();
                    pending.push(Step::Alternative(otherwise, no));
                    if let Some(then) = then {
                        if let Some(yes) = outcome.yes.take() {
                            pending.push(Step::Evaluate(then, yes));
                        }
                        outcome = Condition::default();
                    } else if outcome.yes.is_none() {
                        outcome = Condition::default();
                    }
                }
                Step::Alternative(otherwise, no) => {
                    if let Some(scope) = no {
                        pending.push(Step::Join(std::mem::take(&mut outcome)));
                        pending.push(Step::Evaluate(otherwise, scope));
                    }
                }
                Step::Join(yes) => {
                    outcome.yes = Condition::merge(yes.yes, outcome.yes.take());
                    outcome.no = Condition::merge(yes.no, outcome.no.take());
                    outcome.ty = yes.ty.join(&outcome.ty);
                }
            }
        }
        outcome
    }

    fn coalescing_condition(
        &mut self,
        left: &Expression,
        mut scope: Scope,
    ) -> (Condition, Option<Scope>) {
        let nullish = self.nullish(left, &scope);
        let truth = self.condition_truth(left, &scope);
        let ty = self.expression(left, &mut scope, true);
        if ty == ValueType::Never {
            return (Condition::default(), None);
        }
        if nullish == Some(true) || ty == ValueType::Null {
            return (Condition::default(), Some(scope));
        }
        let fallback = (nullish != Some(false)).then(|| scope.clone());
        self.refine_defined(left, &mut scope);
        let outcome = match truth.or_else(|| assignment_truth(left, &scope)) {
            Some(true) => Condition {
                yes: Some(scope),
                no: None,
                ty,
            },
            Some(false) => Condition {
                yes: None,
                no: Some(scope),
                ty,
            },
            None => Condition {
                yes: Some(scope.clone()),
                no: Some(scope),
                ty,
            },
        };
        (outcome, fallback)
    }

    fn condition_leaf(
        &mut self,
        expression: &Expression,
        mut scope: Scope,
        quiet: bool,
    ) -> Condition {
        let truth = self.condition_truth(expression, &scope);
        let ty = self.expression(expression, &mut scope, quiet);
        if ty == ValueType::Never {
            return Condition::default();
        }
        if let Some(truth) = truth
            .or_else(|| assignment_truth(expression, &scope))
            .or_else(|| (ty == ValueType::Null).then_some(false))
        {
            self.refine(expression, &mut scope, truth);
            return if truth {
                Condition {
                    yes: Some(scope),
                    no: None,
                    ty,
                }
            } else {
                Condition {
                    yes: None,
                    no: Some(scope),
                    ty,
                }
            };
        }
        let mut no = scope.clone();
        self.refine(expression, &mut scope, true);
        self.refine(expression, &mut no, false);
        Condition {
            yes: Some(scope),
            no: Some(no),
            ty,
        }
    }
}

fn assignment_truth(expression: &Expression, scope: &Scope) -> Option<bool> {
    if let ExpressionKind::Assign { target, .. } = &expression.kind {
        return crate::constants::value(target, scope).map(|value| value.truth());
    }
    None
}
