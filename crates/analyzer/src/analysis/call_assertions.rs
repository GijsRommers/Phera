//! `@phpstan-assert User $this->user` can refine a property after a method call.
//! A conditional assertion can refine an argument in `if (Check::hasUser($user))`.

use super::*;
use crate::{
    index::Signature,
    phpdoc::assertions::AssertionBranch,
    scope::{AccessStep, Place},
    types::RelativeTypes,
};

impl Analyzer<'_> {
    pub fn method_assertions(
        &self,
        target: Option<&Expression>,
        receiver: &ValueType,
        name: &MemberName,
        arguments: &Arguments,
        scope: &mut Scope,
        branch: AssertionBranch,
    ) {
        if self.level < 8 {
            return;
        }
        let (Some(class), MemberName::Identifier(name)) = (receiver.class_name(), name) else {
            return;
        };
        let Some((owner, signature)) = self.index.method_owner(class, &name.name) else {
            return;
        };
        let relative = RelativeTypes {
            receiver,
            owner: &owner.name,
            parent: owner.parents.first().map(String::as_str),
        };
        self.apply_assertions(signature, arguments, target, scope, Some(relative), branch);
    }

    pub(super) fn conditional_assertions(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        truth: bool,
    ) {
        if self.level < 8 {
            return;
        }
        let branch = AssertionBranch::OnResult(truth);
        match &expression.kind {
            ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } if let ExpressionKind::Name(class) = &class.kind
                && let Some(class) = self.class_name(class, false) =>
            {
                self.method_assertions(
                    None,
                    &ValueType::Class(class),
                    name,
                    arguments,
                    scope,
                    branch,
                );
            }
            ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                nullsafe: false,
            } if let Some(place) = Place::of(target) => {
                let receiver = self.place_type(&place, scope);
                self.method_assertions(Some(target), &receiver, name, arguments, scope, branch);
            }
            ExpressionKind::Call {
                callee, arguments, ..
            } if let ExpressionKind::Name(name) = &callee.kind
                && callee.span == name.span =>
            {
                if let Some(signature) = self
                    .context
                    .names
                    .resolve(name, UseKind::Function)
                    .iter()
                    .find_map(|name| self.index.functions.get(&name.to_ascii_lowercase()))
                {
                    self.apply_assertions(signature, arguments, None, scope, None, branch);
                }
            }
            _ => {}
        }
    }

    fn apply_assertions(
        &self,
        signature: &Signature,
        arguments: &Arguments,
        receiver: Option<&Expression>,
        scope: &mut Scope,
        relative: Option<RelativeTypes<'_>>,
        branch: AssertionBranch,
    ) {
        let (Some(doc), Arguments::List(arguments)) = (&signature.documentation, arguments) else {
            return;
        };
        for assertion in doc
            .assertions
            .iter()
            .filter(|assertion| assertion.branch == branch)
        {
            let target = if assertion.root == "this" {
                receiver
            } else {
                arguments
                    .iter()
                    .enumerate()
                    .find_map(|(position, argument)| {
                        let parameter = signature.parameter_at(position, argument)?;
                        (!argument.spread && parameter.name == assertion.root)
                            .then_some(&argument.value)
                    })
            };
            if let Some(mut place) = target.and_then(Place::of) {
                place.members.extend(
                    assertion
                        .properties
                        .iter()
                        .cloned()
                        .map(AccessStep::Property),
                );
                let ty = assertion.ty.substitute(&Default::default(), relative);
                if branch == AssertionBranch::Always && !assertion.negated {
                    scope.facts.record(place, ty, None);
                } else {
                    self.refine_place(place, ty, scope, !assertion.negated, None);
                }
            }
        }
    }
}
