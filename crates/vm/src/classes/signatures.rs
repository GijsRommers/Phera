//! Override contracts compare accepted parameter sets and produced return sets at class linking.

use super::{Method, MethodBody};
use crate::{Vm, autoload::MethodVariance};
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
                        declaration: Some(native_parameter_type(parameter)),
                        by_reference: parameter.by_ref,
                        variadic: parameter.variadic,
                        optional: parameter.optional,
                        default_display: native_default(parameter),
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
    fn display(&self, method: &Method, type_name: impl Fn(&str) -> String) -> String {
        let declared = |name: &str| {
            let name = type_name(name);
            if super::magic::MagicMethods::is_linked_magic(&method.name)
                || method.name.starts_with('$')
            {
                ordered_magic_union(&name)
            } else {
                name
            }
        };
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{}{}{}${}{}",
                    parameter
                        .declaration
                        .as_ref()
                        .map(|name| format!("{} ", declared(name)))
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
            super::trait_methods::display(&method.declaring),
            method.name,
            self.returns
                .as_ref()
                .map(|name| format!(": {}", declared(name)))
                .unwrap_or_default()
        )
    }
}

/// Native argument metadata also supplies the spelling used in compatibility errors.
fn native_parameter_type(parameter: &phera_extension_api::Param) -> String {
    let ty = parameter.ty.name();
    if !parameter.nullable || ty == "mixed" {
        ty.into()
    } else if ty.contains('|') {
        format!("{ty}|null")
    } else {
        format!("?{ty}")
    }
}

fn native_default(parameter: &phera_extension_api::Param) -> Option<String> {
    use phera_extension_api::ConstantValue;
    if let Some(name) = parameter.default_name {
        return Some(name.into());
    }
    Some(match parameter.default? {
        ConstantValue::Null => "null".into(),
        ConstantValue::Bool(value) => value.to_string(),
        ConstantValue::Int(value) => value.to_string(),
        ConstantValue::Float(value) => phera_value::float_repr(value),
        ConstantValue::String(value) => format!("{value:?}"),
        ConstantValue::EmptyArray => "[]".into(),
    })
}

impl Vm {
    /// Contravariant parameters may widen inputs; covariant returns may narrow outputs.
    pub(crate) fn check_method_contract(
        &mut self,
        inherited: &Method,
        method: &Method,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<Option<MethodVariance>, NativeError> {
        self.check_contract(inherited, method, None, unit)
    }

    pub(super) fn check_trait_contract(
        &mut self,
        inherited: &Method,
        method: &Method,
        consuming_class: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<Option<MethodVariance>, NativeError> {
        self.check_contract(inherited, method, Some(consuming_class), unit)
    }

    fn check_contract(
        &mut self,
        inherited: &Method,
        method: &Method,
        trait_scope: Option<&str>,
        unit: &phera_bytecode::CompiledUnit,
    ) -> Result<Option<MethodVariance>, NativeError> {
        if (inherited.visibility == phera_bytecode::Visibility::Private && !inherited.is_abstract)
            || (method.name.eq_ignore_ascii_case("__construct") && !inherited.is_abstract)
        {
            return Ok(None);
        }
        let (Some(base), Some(child)) = (
            Contract::from_method(inherited, &self.code),
            Contract::from_method(method, &self.code),
        ) else {
            return Ok(None);
        };
        let inherited_scope = trait_scope.unwrap_or(&inherited.declaring);
        let mut unresolved = Vec::new();
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
            if !self.type_subset(source, inherited_scope, target, &method.declaring, unit) {
                compatible = false;
                unresolved.extend(self.unresolved_contract_types(
                    source,
                    target,
                    &method.declaring,
                    unit,
                ));
            }
        }
        if base.returns.is_some() {
            let source = child.returns.as_deref();
            let target = base.returns.as_deref();
            if source.is_none()
                || !self.type_subset(source, &method.declaring, target, inherited_scope, unit)
            {
                // PHP loads a class that the two return types name before it
                // compares them, for a tentative return type as well.
                let previous_missing = unresolved.len();
                unresolved.extend(self.unresolved_contract_types(
                    source,
                    target,
                    &method.declaring,
                    unit,
                ));
                if base.tentative && compatible && unresolved.len() == previous_missing {
                    if let MethodBody::User(reference) = method.body
                        && !self
                            .code
                            .local(reference)
                            .and_then(|index| {
                                self.code.unit(reference).function_metadata.get(index)
                            })
                            .is_some_and(|metadata| {
                                crate::attribute_diagnostics::attribute(
                                    &metadata.declaration.attributes,
                                    "ReturnTypeWillChange",
                                )
                                .is_some()
                            })
                    {
                        let location = crate::diagnostics::SourceLocation {
                            file: self.code.unit(reference).file.clone(),
                            line: self.code.function(reference).definition_line as usize,
                        };
                        let previous = self.diagnostic_location.replace(location);
                        self.diagnostic("Deprecated", format!(
                            "Return type of {} should either be compatible with {}, or the #[\\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice",
                            child.display(method, |ty| self.contract_type_name(ty, &method.declaring, unit)),
                            base.display(inherited, |ty| self.contract_type_name(ty, inherited_scope, unit))
                        ));
                        self.diagnostic_location = previous;
                    }
                } else {
                    compatible = false;
                }
            }
        }
        if compatible {
            return Ok(None);
        }
        let MethodBody::User(reference) = &method.body else {
            return Ok(None);
        };
        let child = child.display(method, |ty| {
            self.contract_type_name(ty, &method.declaring, unit)
        });
        let base = base.display(inherited, |ty| {
            self.contract_type_name(ty, inherited_scope, unit)
        });
        let file = self.code.unit(*reference).file.clone();
        let line = self.code.function(*reference).definition_line as usize;
        if !unresolved.is_empty() {
            return Ok(Some(MethodVariance {
                names: unresolved.into_iter().map(str::to_owned).collect(),
                child,
                base,
                file,
                line,
            }));
        }
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::error_traces::DeclarationFailure {
                message: format!("Declaration of {child} must be compatible with {base}"),
                file,
                line,
            },
        ))))
    }

    fn contract_type_name(
        &self,
        ty: &str,
        scope: &str,
        unit: &phera_bytecode::CompiledUnit,
    ) -> String {
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
        if ty.split(['?', '|']).any(|member| member == "iterable") {
            crate::types::expanded_iterable_display(&display)
        } else {
            display
        }
    }

    pub(super) fn unresolved_contract_types<'a, 'vm>(
        &'vm self,
        source: Option<&'a str>,
        target: Option<&'a str>,
        declaring: &'a str,
        unit: &'vm phera_bytecode::CompiledUnit,
    ) -> impl Iterator<Item = &'a str> + use<'a, 'vm> {
        let members = |ty: &'a str| ty.split(['?', '|', '&', '(', ')']);
        let may_have_ancestry = |ty: Option<&str>| {
            ty.is_some_and(|ty| {
                ty.split(['?', '|', '&', '(', ')'])
                    .any(|name| name == "iterable" || name == "object" || is_class_type_name(name))
            })
        };
        let comparable = may_have_ancestry(source) && may_have_ancestry(target);
        [source, target]
            .into_iter()
            .flatten()
            .flat_map(members)
            .filter(move |name| {
                comparable
                    && is_class_type_name(name)
                    && self.classes.get(name).is_none()
                    && self
                        .variance_declaration(name, &[declaring], unit)
                        .is_none()
            })
    }

    /// In a class declaration outside an autoloader, a branch that has not
    /// run cannot supply ancestry. Existing autoload cycles still rely on
    /// source declarations until the linker can expose pending class shapes.
    fn variance_declaration<'unit>(
        &self,
        name: &str,
        owners: &[&str],
        unit: &'unit phera_bytecode::CompiledUnit,
    ) -> Option<&'unit phera_bytecode::ClassDeclaration> {
        let in_autoload = !self.autoload.active.is_empty();
        unit.classes.iter().enumerate().find_map(|(index, class)| {
            (class.name.eq_ignore_ascii_case(name)
                && (in_autoload
                    || owners.iter().any(|owner| name.eq_ignore_ascii_case(owner))
                    || unit.hoisted_classes.iter().any(|id| id.index() == index)))
            .then_some(class)
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
        if narrow == wide
            && (narrow_scope == wide_scope
                || !narrow
                    .split(['?', '|', '&', '(', ')'])
                    .any(|member| matches!(member, "self" | "parent" | "static")))
        {
            return true;
        }
        let narrow = contract_alternatives(narrow);
        let wide = contract_alternatives(wide);
        narrow.split('|').all(|source| {
            wide.split('|').any(|target| {
                target.trim_matches(['(', ')']).split('&').all(|bound| {
                    source.trim_matches(['(', ')']).split('&').any(|member| {
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
        if source == "never" || target == "mixed" && source != "void" {
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
            if let Some(class) =
                self.variance_declaration(&name, &[source_scope, target_scope], unit)
            {
                if target == "object"
                    || (target.eq_ignore_ascii_case("Stringable")
                        && class.methods.iter().any(|method| {
                            unit.names[method.name.index()].eq_ignore_ascii_case("__toString")
                        }))
                {
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

/// Traversable and array are separate alternatives in the contract of iterable.
fn contract_alternatives(ty: &str) -> std::borrow::Cow<'_, str> {
    if ty.split(['?', '|']).any(|member| member == "iterable") {
        crate::types::expanded_iterable_display(ty).into()
    } else if let Some(non_null) = ty.strip_prefix('?') {
        format!("null|{non_null}").into()
    } else {
        ty.into()
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

/// PHP's declaration diagnostics list nominal members first and builtins
/// in a fixed order, independent of the written union order.
pub(super) fn ordered_magic_union(ty: &str) -> String {
    if !ty.contains('|') {
        return ty.into();
    }
    let mut members: Vec<_> = ty.split('|').collect();
    members.sort_by_key(|member| match *member {
        "static" => 1,
        "callable" => 2,
        "object" => 3,
        "array" => 4,
        "string" => 5,
        "int" => 6,
        "float" => 7,
        "bool" | "true" | "false" => 8,
        "null" => 9,
        _ => 0,
    });
    members.join("|")
}
