//! Override contracts compare accepted parameter sets and produced return sets at class linking.

use super::{Method, MethodBody};
use crate::Vm;
use phera_extension_api::NativeError;

/// Parameter spelling is retained for PHP's declaration-compatibility diagnostics.
struct ParameterContract {
    name: String,
    declaration: Option<String>,
    by_reference: bool,
    variadic: bool,
    optional: bool,
    /// Original scalar or constant default for PHP diagnostics.
    default_display: Option<String>,
}

/// User signatures carry returns; the native API currently exposes parameters only.
struct Contract {
    parameters: Vec<ParameterContract>,
    returns: Option<String>,
    tentative: bool,
    /// A child must preserve reference returns from its prototype.
    returns_reference: bool,
}

impl Contract {
    /// Copy small declaration metadata without executing default expressions.
    fn from_method(method: &Method, code: &crate::code_table::CodeTable) -> Option<Self> {
        if let MethodBody::Native(entry) = method.body {
            use phera_extension_api::MethodReturn;
            let (returns, tentative) = match entry.return_type? {
                MethodReturn::Required(ty) => (ty, false),
                MethodReturn::Tentative(ty) => (ty, true),
            };
            return Some(Self {
                parameters: entry
                    .params
                    .iter()
                    .map(|parameter| ParameterContract {
                        name: parameter.name.into(),
                        declaration: Some(parameter.ty.name().into()),
                        by_reference: parameter.by_ref,
                        variadic: parameter.variadic,
                        optional: parameter.optional,
                        default_display: None,
                    })
                    .collect(),
                returns: Some(returns.into()),
                tentative,
                returns_reference: false,
            });
        }
        let MethodBody::User(reference) = &method.body else {
            return None;
        };
        let function = code.function(*reference);
        Some(Self {
            returns_reference: function.returns_reference,
            tentative: false,
            parameters: function
                .parameters
                .iter()
                .map(|parameter| ParameterContract {
                    name: code.unit(*reference).names[parameter.name.index()].clone(),
                    declaration: parameter
                        .ty
                        .map(|index| code.unit(*reference).types[index.index()].display.clone()),
                    by_reference: parameter.by_reference,
                    variadic: parameter.variadic,
                    optional: parameter.has_default,
                    default_display: parameter.default_display.clone(),
                })
                .collect(),
            returns: function
                .return_type
                .map(|index| code.unit(*reference).types[index.index()].display.clone()),
        })
    }

    /// PHP lists declaration modifiers before names, then optional defaults and return types.
    fn display(&self, method: &Method) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{}{}{}${}{}",
                    parameter
                        .declaration
                        .as_ref()
                        .map(|name| format!("{name} "))
                        .unwrap_or_default(),
                    if parameter.by_reference { "&" } else { "" },
                    if parameter.variadic { "..." } else { "" },
                    parameter.name,
                    parameter
                        .default_display
                        .as_ref()
                        .map(|value| format!(" = {value}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}{}::{}({parameters}){}",
            if self.returns_reference { "& " } else { "" },
            method.declaring,
            method.name,
            self.returns
                .as_ref()
                .map(|name| format!(": {name}"))
                .unwrap_or_default()
        )
    }
}

impl Vm {
    /// Contravariant parameters may widen inputs; covariant returns may narrow outputs.
    pub(crate) fn check_method_contract(
        &mut self,
        inherited: &Method,
        method: &Method,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<(), NativeError> {
        if inherited.visibility == phera_bytecode::Visibility::Private
            || (method.name.eq_ignore_ascii_case("__construct") && !inherited.is_abstract)
        {
            return Ok(());
        }
        let (Some(base), Some(child)) = (
            Contract::from_method(inherited, &self.code),
            Contract::from_method(method, &self.code),
        ) else {
            return Ok(());
        };
        let mut unresolved = None;
        let mut compatible = !base.returns_reference || child.returns_reference;
        compatible &= child
            .parameters
            .iter()
            .filter(|parameter| !parameter.optional && !parameter.variadic)
            .count()
            <= base
                .parameters
                .iter()
                .filter(|parameter| !parameter.optional && !parameter.variadic)
                .count();
        for (index, parameter) in base.parameters.iter().enumerate() {
            let implementation = child.parameters.get(index).or_else(|| {
                child
                    .parameters
                    .last()
                    .filter(|parameter| parameter.variadic)
            });
            let Some(implementation) = implementation else {
                compatible = false;
                break;
            };
            compatible &= parameter.by_reference == implementation.by_reference;
            compatible &= !parameter.variadic || implementation.variadic;
            compatible &= !parameter.optional || implementation.optional || implementation.variadic;
            let source = parameter.declaration.as_deref();
            let target = implementation.declaration.as_deref();
            if !self.type_subset(
                source,
                &inherited.declaring,
                target,
                &method.declaring,
                unit,
            ) {
                compatible = false;
                unresolved =
                    unresolved.or_else(|| self.unresolved_contract_type(source, target, unit));
            }
        }
        if base.returns.is_some() && (!base.tentative || child.returns.is_some()) {
            let source = child.returns.as_deref();
            let target = base.returns.as_deref();
            if !self.type_subset(
                source,
                &method.declaring,
                target,
                &inherited.declaring,
                unit,
            ) {
                if base.tentative && compatible {
                    if let MethodBody::User(reference) = method.body {
                        let location = crate::diagnostics::SourceLocation {
                            file: self.code.unit(reference).file.clone(),
                            line: self.code.function(reference).definition_line as usize,
                        };
                        let previous = self.diagnostic_location.replace(location);
                        self.diagnostic("Deprecated", format!(
                            "Return type of {} should either be compatible with {}, or the #[\\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice",
                            child.display(method), base.display(inherited)
                        ));
                        self.diagnostic_location = previous;
                    }
                } else {
                    compatible = false;
                    unresolved =
                        unresolved.or_else(|| self.unresolved_contract_type(source, target, unit));
                }
            }
        }
        if compatible {
            return Ok(());
        }
        let MethodBody::User(reference) = &method.body else {
            return Ok(());
        };
        let child = child.display(method);
        let base = base.display(inherited);
        let file = self.code.unit(*reference).file.clone();
        let line = self.code.function(*reference).definition_line as usize;
        if let Some(name) = unresolved {
            let failure = crate::error_traces::DeclarationFailure {
                message: format!(
                    "Could not check compatibility between {child} and {base}, because class {name} is not available"
                ),
                file,
                line,
            };
            return Err(crate::autoload::ClassMiss::signature(name, failure));
        }
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::error_traces::DeclarationFailure {
                message: format!("Declaration of {child} must be compatible with {base}"),
                file,
                line,
            },
        ))))
    }

    /// Linking a method signature can require class identities even though passing arguments does not.
    fn unresolved_contract_type<'a>(
        &self,
        source: Option<&'a str>,
        target: Option<&'a str>,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Option<&'a str> {
        let members = |ty: &'a str| ty.split(['?', '|', '&', '(', ')']);
        let may_have_ancestry = |ty: Option<&str>| {
            ty.is_some_and(|ty| {
                ty.split(['?', '|', '&', '(', ')'])
                    .any(|name| name == "iterable" || name == "object" || is_class_type_name(name))
            })
        };
        if !may_have_ancestry(source) || !may_have_ancestry(target) {
            return None;
        }
        [source, target]
            .into_iter()
            .flatten()
            .flat_map(members)
            .find(|name| {
                is_class_type_name(name)
                    && self.classes.get(name).is_none()
                    && !unit
                        .classes
                        .iter()
                        .any(|class| class.name.eq_ignore_ascii_case(name))
            })
    }

    /// Union members must each fit some destination member; intersections require all bounds.
    pub(super) fn type_subset(
        &self,
        narrow: Option<&str>,
        narrow_scope: &str,
        wide: Option<&str>,
        wide_scope: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> bool {
        let Some(wide) = wide else {
            return true;
        };
        let Some(narrow) = narrow else {
            return wide == "mixed";
        };
        let narrow = narrow.replace('?', "null|");
        let wide = wide.replace('?', "null|");
        narrow.split('|').all(|source| {
            wide.split('|').any(|target| {
                target.split('&').all(|bound| {
                    source.split('&').any(|member| {
                        self.atomic_subtype(member, narrow_scope, bound, wide_scope, unit)
                    })
                })
            })
        })
    }

    /// The class being linked is not published yet, so its parent supplies ancestry facts.
    fn atomic_subtype(
        &self,
        source: &str,
        source_scope: &str,
        target: &str,
        target_scope: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> bool {
        if target == "mixed" || source == "never" {
            return true;
        }
        if target == "static" {
            return source == "static"
                || (source == "self" || source.eq_ignore_ascii_case(source_scope))
                    && unit.classes.iter().any(|class| {
                        class.name.eq_ignore_ascii_case(source_scope) && class.is_final
                    });
        }
        let source_name = self.resolve_signature_keyword(source, source_scope, unit);
        let target_name = self.resolve_signature_keyword(target, target_scope, unit);
        let source = source_name.as_str();
        let target = target_name.as_str();
        if source.eq_ignore_ascii_case(target) {
            return true;
        }
        if target == "bool" && matches!(source, "true" | "false") {
            return true;
        }
        if target == "iterable" && source == "array" {
            return true;
        }
        let target = if target == "iterable" {
            "Traversable"
        } else {
            target
        };
        if target == "callable" && source == "Closure" {
            return true;
        }
        let mut pending = vec![source.to_owned()];
        let mut visited = std::collections::HashSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name.to_ascii_lowercase()) {
                continue;
            }
            if name.eq_ignore_ascii_case(target) {
                return true;
            }
            if let Some(class) = self.classes.get(&name)
                && (target == "object" || self.class_matches(&class, target))
            {
                return true;
            }
            if let Some(class) = unit
                .classes
                .iter()
                .find(|class| class.name.eq_ignore_ascii_case(&name))
            {
                if target == "object" {
                    return true;
                }
                pending.extend(
                    class
                        .parent
                        .iter()
                        .chain(&class.interfaces)
                        .map(|symbol| unit.symbols[symbol.index()].written.clone()),
                );
            }
        }
        false
    }

    /// Relative declaration types refer to the method's declaring class, including ancestors.
    fn resolve_signature_keyword(
        &self,
        name: &str,
        scope: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> String {
        match name {
            "self" | "static" => scope.to_owned(),
            "parent" => self
                .classes
                .get(scope)
                .and_then(|class| class.parent.clone())
                .map(|class| class.name.clone())
                .or_else(|| {
                    unit.classes
                        .iter()
                        .find(|class| class.name.eq_ignore_ascii_case(scope))
                        .and_then(|class| class.parent)
                        .map(|symbol| unit.symbols[symbol.index()].written.clone())
                })
                .unwrap_or_else(|| name.to_owned()),
            _ => name.to_owned(),
        }
    }
}

/// Native type spellings cannot be supplied by an autoloader.
fn is_class_type_name(name: &str) -> bool {
    !matches!(
        name,
        "" | "null"
            | "true"
            | "false"
            | "bool"
            | "int"
            | "float"
            | "string"
            | "array"
            | "object"
            | "mixed"
            | "iterable"
            | "callable"
            | "void"
            | "never"
            | "self"
            | "parent"
            | "static"
    )
}
