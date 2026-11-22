//! Read a method's declared types without copying its signature.
//!
//! A method such as `function make(?Task $task = null): Task` already
//! has its parameter flags and type spellings in immutable bytecode.

use std::borrow::Cow;

use super::super::{Method, MethodBody};

/// Parameter spelling is retained for PHP's declaration-compatibility diagnostics.
pub(super) struct ParameterContract<'a> {
    name: &'a str,
    pub(super) declaration: Option<Cow<'a, str>>,
    pub(super) by_reference: bool,
    pub(super) variadic: bool,
    pub(super) optional: bool,
    default: DefaultValue<'a>,
}

/// The original scalar or constant default, rendered only in diagnostics.
enum DefaultValue<'a> {
    User(Option<&'a str>),
    Native(&'a phera_extension_api::Param),
}

impl DefaultValue<'_> {
    fn display(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::User(value) => value.map(Cow::Borrowed),
            Self::Native(parameter) => native_default(parameter).map(Cow::Owned),
        }
    }
}

pub(super) enum Parameters<'a> {
    User(
        &'a [phera_bytecode::Parameter],
        &'a phera_bytecode::CompiledUnit,
    ),
    Native(&'a [phera_extension_api::Param]),
}

impl<'a> Parameters<'a> {
    fn len(&self) -> usize {
        match self {
            Self::User(parameters, _) => parameters.len(),
            Self::Native(parameters) => parameters.len(),
        }
    }

    pub(super) fn required(&self) -> usize {
        match self {
            Self::User(parameters, _) => parameters
                .iter()
                .filter(|parameter| !parameter.has_default && !parameter.variadic)
                .count(),
            Self::Native(parameters) => parameters
                .iter()
                .filter(|parameter| !parameter.optional && !parameter.variadic)
                .count(),
        }
    }

    pub(super) fn get(&self, index: usize) -> Option<ParameterContract<'a>> {
        match self {
            Self::User(parameters, unit) => {
                parameters.get(index).map(|parameter| ParameterContract {
                    name: &unit.names[parameter.name.index()],
                    declaration: parameter
                        .ty
                        .map(|id| Cow::Borrowed(unit.types[id.index()].display.as_str())),
                    by_reference: parameter.by_reference,
                    variadic: parameter.variadic,
                    optional: parameter.has_default,
                    default: DefaultValue::User(parameter.default_display.as_deref()),
                })
            }
            Self::Native(parameters) => parameters.get(index).map(|parameter| ParameterContract {
                name: parameter.name,
                declaration: Some(native_parameter_type(parameter)),
                by_reference: parameter.by_ref,
                variadic: parameter.variadic,
                optional: parameter.optional,
                default: DefaultValue::Native(parameter),
            }),
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = ParameterContract<'a>> {
        (0..self.len()).filter_map(|index| self.get(index))
    }

    pub(super) fn last(&self) -> Option<ParameterContract<'a>> {
        self.get(self.len().checked_sub(1)?)
    }
}

/// User signatures carry returns; the native API currently exposes parameters only.
pub(super) struct Contract<'a> {
    pub(super) parameters: Parameters<'a>,
    pub(super) returns: Option<&'a str>,
    pub(super) tentative: bool,
    /// A child must preserve reference returns from its prototype.
    pub(super) returns_reference: bool,
}

impl<'a> Contract<'a> {
    /// Read immutable declaration metadata without executing default expressions.
    pub(super) fn from_method(
        method: &Method,
        code: &'a crate::code_table::CodeTable,
    ) -> Option<Self> {
        if let MethodBody::Native(entry) = method.body {
            use phera_extension_api::MethodReturn;
            let (returns, tentative) = match entry.return_type? {
                MethodReturn::Required(ty) => (ty, false),
                MethodReturn::Tentative(ty) => (ty, true),
            };
            return Some(Self {
                parameters: Parameters::Native(entry.params),
                returns: Some(returns),
                tentative,
                returns_reference: false,
            });
        }
        let MethodBody::User(reference) = &method.body else {
            return None;
        };
        let function = code.function(*reference);
        let unit = code.unit(*reference);
        Some(Self {
            returns_reference: function.returns_reference,
            tentative: false,
            parameters: Parameters::User(&function.parameters, unit),
            returns: function
                .return_type
                .map(|index| unit.types[index.index()].display.as_str()),
        })
    }

    /// PHP lists declaration modifiers before names, then optional defaults and return types.
    pub(super) fn display(&self, method: &Method, type_name: impl Fn(&str) -> String) -> String {
        let declared = |name: &str| {
            let name = type_name(name);
            if super::super::magic::MagicMethods::is_linked_magic(&method.name)
                || method.name.starts_with('$')
            {
                super::ordered_magic_union(&name)
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
                        .default
                        .display()
                        .map(|value| format!(" = {value}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}{}::{}({parameters}){}",
            if self.returns_reference { "& " } else { "" },
            super::super::trait_methods::display(&method.declaring),
            method.name,
            self.returns
                .map(|name| format!(": {}", declared(name)))
                .unwrap_or_default()
        )
    }
}

/// Native argument metadata also supplies the spelling used in compatibility errors.
fn native_parameter_type(parameter: &phera_extension_api::Param) -> Cow<'static, str> {
    let ty = parameter.ty.name();
    if !parameter.nullable || ty == "mixed" {
        ty.into()
    } else if ty.contains('|') {
        format!("{ty}|null").into()
    } else {
        format!("?{ty}").into()
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
