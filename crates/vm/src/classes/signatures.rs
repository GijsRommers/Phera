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
    /// A child must preserve reference returns from its prototype.
    returns_reference: bool,
}

impl Contract {
    /// Copy small declaration metadata without executing default expressions.
    fn from_method(method: &Method, code: &crate::code_table::CodeTable) -> Option<Self> {
        let MethodBody::User(reference) = &method.body else {
            return None;
        };
        let function = code.function(*reference);
        Some(Self {
            returns_reference: function.returns_reference,
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
    fn display(&self, method: &Method, type_name: impl Fn(&str) -> String) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{}{}{}${}{}",
                    parameter
                        .declaration
                        .as_ref()
                        .map(|name| format!("{} ", type_name(name)))
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
                .map(|name| format!(": {}", type_name(name)))
                .unwrap_or_default()
        )
    }
}

impl Vm {
    /// Contravariant parameters may widen inputs; covariant returns may narrow outputs.
    pub(crate) fn check_method_contract(
        &self,
        inherited: &Method,
        method: &Method,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<(), NativeError> {
        self.check_contract(inherited, method, None, unit)
    }

    pub(super) fn check_trait_contract(
        &self,
        inherited: &Method,
        method: &Method,
        consuming_class: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<(), NativeError> {
        self.check_contract(inherited, method, Some(consuming_class), unit)
    }

    fn check_contract(
        &self,
        inherited: &Method,
        method: &Method,
        trait_scope: Option<&str>,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<(), NativeError> {
        if (inherited.visibility == phera_bytecode::Visibility::Private && !inherited.is_abstract)
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
        let inherited_scope = trait_scope.unwrap_or(&inherited.declaring);
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
            compatible &= self.type_subset(
                parameter.declaration.as_deref(),
                inherited_scope,
                implementation.declaration.as_deref(),
                &method.declaring,
                unit,
            );
        }
        if base.returns.is_some() {
            compatible &= self.type_subset(
                child.returns.as_deref(),
                &method.declaring,
                base.returns.as_deref(),
                inherited_scope,
                unit,
            );
        }
        if compatible {
            return Ok(());
        }
        let MethodBody::User(reference) = &method.body else {
            return Ok(());
        };
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::error_traces::DeclarationFailure {
                message: format!(
                    "Declaration of {} must be compatible with {}",
                    child.display(method, |ty| self.contract_type_name(
                        ty,
                        &method.declaring,
                        trait_scope,
                        unit
                    )),
                    base.display(inherited, |ty| self.contract_type_name(
                        ty,
                        inherited_scope,
                        trait_scope,
                        unit
                    ))
                ),
                file: self.code.unit(*reference).file.clone(),
                line: self.code.function(*reference).definition_line as usize,
            },
        ))))
    }

    fn contract_type_name(
        &self,
        ty: &str,
        scope: &str,
        trait_scope: Option<&str>,
        unit: &phera_bytecode::CompiledUnit,
    ) -> String {
        if trait_scope.is_none() {
            return ty.to_owned();
        }
        let separators = ['|', '&', '?', '(', ')'];
        let mut display = String::new();
        for segment in ty.split_inclusive(separators) {
            let name = segment.trim_end_matches(separators);
            if matches!(name, "self" | "parent") {
                display.push_str(&self.resolve_signature_keyword(name, scope, unit));
                display.push_str(&segment[name.len()..]);
            } else {
                display.push_str(segment);
            }
        }
        display
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
        if target == "iterable" && matches!(source, "array" | "Traversable") {
            return true;
        }
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
                && (target == "object" || class.is_a(target))
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
