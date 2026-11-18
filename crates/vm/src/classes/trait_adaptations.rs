//! `T::m insteadof U` excludes U's original method while `U::m as other` still imports its alias.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::{ClassDeclaration, TraitAdaptation, Visibility};
use phera_extension_api::NativeError;

use super::{Class, trait_methods::display};
use crate::Vm;

#[derive(Default)]
pub(super) struct MethodRules {
    excluded: HashSet<(String, String)>,
    aliases: Vec<Alias>,
}

pub(super) struct Alias {
    trait_name: String,
    method: String,
    pub(super) name: Option<String>,
    pub(super) visibility: Option<Visibility>,
    pub(super) is_final: bool,
}

impl MethodRules {
    pub(super) fn excludes(&self, provider: &str, method: &str) -> bool {
        self.excluded
            .iter()
            .any(|(trait_name, key)| trait_name == provider && key == method)
    }

    pub(super) fn aliases<'a>(
        &'a self,
        provider: &'a str,
        method: &'a str,
    ) -> impl Iterator<Item = &'a Alias> {
        self.aliases
            .iter()
            .filter(move |alias| alias.trait_name == provider && alias.method == method)
    }
}

impl Vm {
    pub(super) fn trait_method_rules(
        &self,
        declaration: &ClassDeclaration,
        traits: &[Rc<Class>],
    ) -> Result<MethodRules, NativeError> {
        let mut rules = MethodRules::default();
        let class = display(&declaration.name);
        let required = |name: &str| {
            let Some(dependency) = self.classes.get(name) else {
                return Err(NativeError::error(format!("Could not find trait {name}")));
            };
            if dependency.kind != phera_bytecode::ClassKind::Trait {
                return Err(NativeError::error(format!(
                    "Class {} is not a trait, Only traits may be used in 'as' and 'insteadof' statements",
                    dependency.name,
                )));
            }
            if !traits
                .iter()
                .any(|trait_class| Rc::ptr_eq(trait_class, &dependency))
            {
                return Err(NativeError::error(format!(
                    "Required Trait {} wasn't added to {class}",
                    dependency.name
                )));
            }
            Ok(dependency)
        };
        for rule in declaration
            .traits
            .iter()
            .flat_map(|usage| &usage.adaptations)
        {
            match rule {
                TraitAdaptation::Precedence {
                    trait_name,
                    method,
                    instead_of,
                } => {
                    let winner = required(trait_name)?;
                    let key = method.to_ascii_lowercase();
                    if !winner.methods.contains_key(&key) {
                        return Err(NativeError::error(format!(
                            "A precedence rule was defined for {}::{method} but this method does not exist",
                            winner.name,
                        )));
                    }
                    for name in instead_of {
                        let excluded = required(name)?;
                        if Rc::ptr_eq(&winner, &excluded) {
                            return Err(NativeError::error(format!(
                                "Inconsistent insteadof definition. The method {method} is to be used from {}, but {} is also on the exclude list",
                                winner.name, excluded.name,
                            )));
                        }
                        if !rules
                            .excluded
                            .insert((excluded.name.to_ascii_lowercase(), key.clone()))
                        {
                            return Err(NativeError::error(format!(
                                "Failed to evaluate a trait precedence ({method}). Method of trait {name} was defined to be excluded multiple times"
                            )));
                        }
                    }
                }
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                    is_final,
                } => {
                    let key = method.to_ascii_lowercase();
                    let provider = if let Some(name) = trait_name {
                        let provider = required(name)?;
                        if !provider.methods.contains_key(&key) {
                            return Err(NativeError::error(format!(
                                "An alias was defined for {}::{method} but this method does not exist",
                                provider.name,
                            )));
                        }
                        provider
                    } else {
                        alias_provider(traits, method, &key, alias.as_deref())?
                    };
                    rules.aliases.push(Alias {
                        trait_name: provider.name.to_ascii_lowercase(),
                        method: key,
                        name: alias.clone(),
                        visibility: *visibility,
                        is_final: *is_final,
                    });
                }
            }
        }
        Ok(rules)
    }
}

fn alias_provider(
    traits: &[Rc<Class>],
    method: &str,
    key: &str,
    alias: Option<&str>,
) -> Result<Rc<Class>, NativeError> {
    let mut providers = traits
        .iter()
        .filter(|provider| provider.methods.contains_key(key));
    let Some(first) = providers.next() else {
        let message = match alias {
            Some(alias) => format!(
                "An alias ({alias}) was defined for method {method}(), but this method does not exist"
            ),
            None => format!(
                "The modifiers of the trait method {method}() are changed, but this method does not exist. Error"
            ),
        };
        return Err(NativeError::error(message));
    };
    if let Some(second) = providers.next() {
        return Err(NativeError::error(format!(
            "An alias was defined for method {method}(), which exists in both {} and {}. Use {}::{method} or {}::{method} to resolve the ambiguity",
            first.name, second.name, first.name, second.name,
        )));
    }
    Ok(first.clone())
}
