//! After `$collection->initialize()`, its `@phpstan-assert` can refine a property.

use super::*;
use crate::{
    index::Signature,
    scope::{AccessStep, Place},
    types::RelativeTypes,
};

impl Analyzer<'_> {
    pub fn method_assertions(
        &self,
        target: &Expression,
        receiver: &ValueType,
        name: &MemberName,
        arguments: &Arguments,
        scope: &mut Scope,
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
        apply_assertions(signature, arguments, Some(target), scope, Some(relative));
    }
}

fn apply_assertions(
    signature: &Signature,
    arguments: &Arguments,
    receiver: Option<&Expression>,
    scope: &mut Scope,
    relative: Option<RelativeTypes<'_>>,
) {
    let (Some(doc), Arguments::List(arguments)) = (&signature.documentation, arguments) else {
        return;
    };
    for assertion in &doc.assertions {
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
            scope.facts.record(place, ty, None);
        }
    }
}
