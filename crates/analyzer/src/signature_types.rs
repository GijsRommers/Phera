//! A call to `/** @return User */ function load()` has result type User.
//! The declaration retains its native contract for validation at the source location.

use crate::{
    index::{ClassInfo, Index, PropertyInfo, Signature},
    types::{Compatibility, TypePolicy, ValueType},
};
use phera_ast::{Argument, Arguments};
use std::collections::{BTreeMap, BTreeSet};

impl Signature {
    pub fn typed(mut self, index: &Index) -> Self {
        if let Some(doc) = &self.documentation {
            for parameter in &mut self.parameters {
                if let Some(annotation) = doc.params.get(&parameter.name) {
                    parameter.ty = refine(&parameter.ty, &annotation.ty, index);
                }
            }
            if let Some(annotation) = &doc.returns {
                self.returns = refine(&self.returns, &annotation.ty, index);
            }
        }
        self
    }

    pub fn for_receiver(mut self, index: &Index, owner: &ClassInfo, receiver: &ValueType) -> Self {
        let mut bindings = if owner.documented.template_names.is_empty() {
            BTreeMap::new()
        } else {
            class_bindings(index, &owner.name, receiver)
        };
        if let Some(doc) = &self.documentation {
            // A method may declare its own template with the same name.
            for name in &doc.template_names {
                bindings.remove(name);
            }
        }
        if !bindings.is_empty() {
            for parameter in &mut self.parameters {
                parameter.ty = parameter.ty.substitute(&bindings, Some(receiver));
            }
        }
        if self.documentation.is_some()
            || matches!(&self.returns, ValueType::Class(name) if name == "static")
        {
            self.returns = self.returns.substitute(&bindings, Some(receiver));
        }
        self
    }

    pub fn infer_return(
        &self,
        arguments: &Arguments,
        types: &[ValueType],
        index: &Index,
    ) -> ValueType {
        let (Some(doc), Arguments::List(arguments)) = (&self.documentation, arguments) else {
            return self.returns.clone();
        };
        if (doc.template_names.is_empty() && !matches!(self.returns, ValueType::Conditional(_)))
            || (types.is_empty() && !arguments.is_empty())
        {
            return self.returns.clone();
        }
        let mut inferred = BTreeMap::new();
        let mut actual = BTreeMap::new();
        for (position, (argument, given)) in arguments.iter().zip(types).enumerate() {
            if argument.spread {
                continue;
            }
            if let Some(parameter) = self.parameter_at(position, argument) {
                actual.insert(parameter.name.as_str(), given);
                infer_templates(
                    &parameter.ty,
                    given,
                    &doc.template_names,
                    &mut inferred,
                    index,
                );
            }
        }
        let resolved = self.returns.substitute(&inferred, None);
        if let ValueType::Conditional(conditional) = &resolved {
            let subject = if let Some(name) = &conditional.variable {
                actual.get(name.as_str()).copied().or_else(|| {
                    (!arguments.iter().any(|argument| argument.spread)
                        && self
                            .parameters
                            .iter()
                            .any(|parameter| parameter.name == *name && parameter.default_is_null))
                    .then_some(&ValueType::Null)
                })
            } else {
                Some(&conditional.subject)
            };
            if let Some(subject) = subject {
                let relation =
                    conditional
                        .tested
                        .accepts(subject, index, TypePolicy::Documentation);
                let selection = match relation {
                    Compatibility::Accepts => Some(true),
                    Compatibility::Rejects
                        if (conditional.tested == ValueType::Null && subject.excludes_null())
                            || subject == &ValueType::Null =>
                    {
                        Some(false)
                    }
                    _ => None,
                };
                if let Some(matches) = selection {
                    return if matches == (conditional.test == crate::types::ConditionTest::Is) {
                        conditional.yes.clone()
                    } else {
                        conditional.no.clone()
                    };
                }
            }
        }
        resolved
    }

    pub fn parameter_at(
        &self,
        position: usize,
        argument: &Argument,
    ) -> Option<&crate::index::ParameterInfo> {
        let parameter = if let Some(name) = &argument.name {
            self.parameters
                .iter()
                .find(|parameter| parameter.name == name.name)
        } else {
            self.parameters.get(position)
        };
        parameter.or_else(|| {
            self.parameters
                .last()
                .filter(|parameter| parameter.variadic)
        })
    }
}

fn infer_templates(
    expected: &ValueType,
    given: &ValueType,
    variables: &[String],
    bindings: &mut BTreeMap<String, ValueType>,
    index: &Index,
) {
    use ValueType as T;
    let mut pending = vec![(expected, given)];
    while let Some((expected, given)) = pending.pop() {
        match (expected, given) {
            (T::Template(template), _)
                if variables.contains(&template.name)
                    && template
                        .bound
                        .accepts(given, index, TypePolicy::Documentation)
                        == Compatibility::Accepts =>
            {
                bindings
                    .entry(template.name.clone())
                    .and_modify(|old| *old = old.join(given))
                    .or_insert_with(|| given.clone());
            }
            (T::ClassString(expected), T::ClassString(given)) => pending.push((expected, given)),
            (T::Collection(expected), T::Collection(given)) => {
                pending.extend([(&expected.key, &given.key), (&expected.value, &given.value)])
            }
            (T::Collection(expected), T::Shape(given)) => pending.extend(
                given
                    .fields
                    .iter()
                    .map(|field| (&expected.value, &field.value)),
            ),
            (T::Generic(expected), T::Generic(given))
                if expected.base.eq_ignore_ascii_case(&given.base) =>
            {
                pending.extend(expected.arguments.iter().zip(&given.arguments));
            }
            _ => {}
        }
    }
}

fn class_bindings(index: &Index, owner: &str, receiver: &ValueType) -> BTreeMap<String, ValueType> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![receiver.clone()];
    while let Some(ty) = pending.pop() {
        let Some(class) = ty.class_name().and_then(|name| index.class(name)) else {
            continue;
        };
        if !seen.insert(&class.name) {
            continue;
        }
        let bindings = if let ValueType::Generic(generic) = &ty {
            class
                .documented
                .template_names
                .iter()
                .cloned()
                .zip(generic.arguments.iter().cloned())
                .collect()
        } else {
            BTreeMap::new()
        };
        if class.name.eq_ignore_ascii_case(owner) {
            return bindings;
        }
        for base in class
            .parents
            .iter()
            .chain(&class.interfaces)
            .chain(&class.traits)
        {
            let documented = class.documented.generic_bases.iter().find(|ty| {
                ty.class_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(base))
            });
            pending.push(documented.map_or_else(
                || ValueType::Class(base.clone()),
                |ty| ty.substitute(&bindings, Some(receiver)),
            ));
        }
    }
    BTreeMap::new()
}

impl PropertyInfo {
    pub fn effective_type(&self, index: &Index) -> ValueType {
        self.documented_type.as_ref().map_or_else(
            || self.ty.clone(),
            |annotation| refine(&self.ty, annotation, index),
        )
    }
}

fn refine(native: &ValueType, documented: &ValueType, index: &Index) -> ValueType {
    if native.accepts(documented, index, TypePolicy::Documentation) == Compatibility::Accepts {
        documented.clone()
    } else {
        native.clone()
    }
}
