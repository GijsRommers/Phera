//! `enum State: int { case Ready = 1; }` links an implicit interface and static methods.

use std::{collections::HashMap, rc::Rc};

use phera_bytecode::{ClassDeclaration, ClassKind, CompiledUnit, EnumBacking, Visibility};
use phera_extension_api::{ClassEntry, MethodEntry, NativeError, Param, ParamType};
use phera_value::{ArrayKey, Value};

use super::{
    Class, LinkedConstant, Method, MethodBody,
    class::{ConstantInitializer, PropertyDefault},
};
use crate::Vm;

const VALUE_PARAMETER: Param = Param::new("value", ParamType::IntOrString);
pub(super) const INTERFACES: &[ClassEntry] = &[
    ClassEntry {
        methods: &[MethodEntry::required("cases", &[])
            .static_()
            .returning("array")],
        ..ClassEntry::interface("UnitEnum")
    },
    ClassEntry {
        interfaces: &["UnitEnum"],
        methods: &[
            MethodEntry::required("from", &[VALUE_PARAMETER])
                .static_()
                .returning("static"),
            MethodEntry::required("tryFrom", &[VALUE_PARAMETER])
                .static_()
                .returning("?static"),
        ],
        ..ClassEntry::interface("BackedEnum")
    },
];

/// A native static member of a linked enum.
#[derive(Clone, Copy)]
pub(crate) enum EnumMethod {
    Cases,
    From,
    TryFrom,
}

impl EnumMethod {
    pub(crate) fn parameters(self) -> &'static [Param] {
        match self {
            Self::Cases => &[],
            Self::From | Self::TryFrom => &[VALUE_PARAMETER],
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cases => "cases",
            Self::From => "from",
            Self::TryFrom => "tryFrom",
        }
    }
}

pub(crate) struct EnumInfo {
    pub(crate) backing: Option<EnumBacking>,
    pub(super) names: Vec<String>,
}

pub(crate) use super::enum_cases::EnumCases;

impl Vm {
    /// Enums inherit their implicit interface, which ordinary classes may not implement.
    pub(super) fn link_enum_interfaces(
        &self,
        declaration: &ClassDeclaration,
        interfaces: &mut Vec<Rc<Class>>,
    ) -> Result<(), NativeError> {
        let name = &declaration.name;
        for interface in &*interfaces {
            if !interface.is_a("UnitEnum") {
                continue;
            }
            if declaration.kind != ClassKind::Enum {
                let forbidden = if interface.is_a("BackedEnum") {
                    "BackedEnum"
                } else {
                    "UnitEnum"
                };
                return Err(NativeError::error(format!(
                    "Non-enum class {name} cannot implement interface {forbidden}"
                )));
            }
            if interface.is_a("BackedEnum") && declaration.enum_backing.is_none() {
                return Err(NativeError::error(format!(
                    "Non-backed enum {name} cannot implement interface BackedEnum"
                )));
            }
            if matches!(interface.name.as_str(), "UnitEnum" | "BackedEnum") {
                return Err(NativeError::error(format!(
                    "Enum {name} cannot implement previously implemented interface {}",
                    interface.name
                )));
            }
        }
        if declaration.kind == ClassKind::Enum {
            let mut implemented = std::collections::HashSet::new();
            for interface in interfaces.iter() {
                if !implemented.insert(interface.index) {
                    return Err(NativeError::error(format!(
                        "Enum {name} cannot implement previously implemented interface {}",
                        interface.name,
                    )));
                }
            }
            for name in ["UnitEnum"]
                .into_iter()
                .chain(declaration.enum_backing.map(|_| "BackedEnum"))
            {
                let interface = self
                    .classes
                    .get(name)
                    .ok_or_else(|| internal_error!("enum interface"))?;
                interfaces.push(interface);
            }
        }
        Ok(())
    }

    pub(super) fn install_enum_methods(
        &self,
        declaration: &ClassDeclaration,
        methods: &mut HashMap<String, Rc<Method>>,
    ) -> Result<Option<EnumInfo>, NativeError> {
        if declaration.kind != ClassKind::Enum {
            return Ok(None);
        }
        let name = &declaration.name;
        let backing = declaration.enum_backing;
        let backed = backing
            .into_iter()
            .flat_map(|_| [EnumMethod::From, EnumMethod::TryFrom]);
        for operation in [EnumMethod::Cases].into_iter().chain(backed) {
            let method = operation.name();
            let key = method.to_ascii_lowercase();
            if methods.contains_key(&key) {
                return Err(NativeError::error(format!(
                    "Cannot redeclare {name}::{method}()"
                )));
            }
            methods.insert(
                key,
                Rc::new(Method {
                    signature: std::cell::OnceCell::new(),
                    name: method.to_owned(),
                    declaring: name.clone(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_final: false,
                    is_abstract: false,
                    body: MethodBody::Enum(operation),
                }),
            );
        }
        Ok(Some(EnumInfo {
            backing,
            names: Vec::new(),
        }))
    }

    pub(super) fn link_enum_members(
        &mut self,
        declaration: &ClassDeclaration,
        unit: &std::sync::Arc<CompiledUnit>,
        info: &mut EnumInfo,
        defaults: &mut Vec<PropertyDefault>,
        statics: &[PropertyDefault],
        constants: &mut HashMap<String, Rc<LinkedConstant>>,
    ) -> Result<(), NativeError> {
        let class = &declaration.name;
        if !defaults.is_empty() || !statics.is_empty() {
            return Err(NativeError::error(format!(
                "Enum {class} cannot include properties"
            )));
        }
        for name in ["name"].into_iter().chain(info.backing.map(|_| "value")) {
            defaults.push(enum_property(class, name));
        }
        for case in &declaration.enum_cases {
            let name = &unit.names[case.name.index()];
            if constants
                .get(name)
                .is_some_and(|constant| constant.declaring == *class)
            {
                return Err(NativeError::error(format!(
                    "Cannot redefine class constant {class}::{name}"
                )));
            }
            info.names.push(name.clone());
            let deprecated = unit
                .has_link_attributes
                .then(|| {
                    crate::attribute_diagnostics::attribute(&case.metadata.attributes, "Deprecated")
                        .map(|attribute| {
                            crate::attribute_diagnostics::deprecation_message(
                                attribute,
                                &format!("Enum case {class}::{name}"),
                            )
                        })
                })
                .flatten();
            constants.insert(
                name.clone(),
                Rc::new(LinkedConstant {
                    deprecated,
                    declaring: class.clone(),
                    visibility: Visibility::Public,
                    is_final: false,
                    initializer: ConstantInitializer::EnumCase {
                        name: name.clone(),
                        backing: case
                            .backing
                            .map(|function| self.code.index_in(unit, Some(function.index()))),
                        slot: Some(self.class_states.new_constant()),
                    },
                    ty: None,
                }),
            );
        }
        Ok(())
    }
}

/// Internal unit enums share the same case allocation and method dispatch as PHP enums.
pub(super) fn native_unit_enum(
    class: &str,
    cases: &[&str],
    methods: &mut HashMap<String, Rc<Method>>,
    defaults: &mut Vec<PropertyDefault>,
    constants: &mut HashMap<String, Rc<LinkedConstant>>,
) -> EnumInfo {
    methods.insert(
        "cases".into(),
        Rc::new(Method {
            signature: std::cell::OnceCell::new(),
            name: "cases".into(),
            declaring: class.into(),
            visibility: Visibility::Public,
            is_static: true,
            is_final: false,
            is_abstract: false,
            body: MethodBody::Enum(EnumMethod::Cases),
        }),
    );
    defaults.push(enum_property(class, "name"));
    for &name in cases {
        constants.insert(
            name.into(),
            Rc::new(LinkedConstant {
                deprecated: None,
                declaring: class.into(),
                visibility: Visibility::Public,
                is_final: false,
                initializer: ConstantInitializer::EnumCase {
                    name: name.into(),
                    backing: None,
                    slot: None,
                },
                ty: None,
            }),
        );
    }
    EnumInfo {
        backing: None,
        names: cases.iter().map(|name| (*name).to_owned()).collect(),
    }
}

pub(super) fn enum_property(class: &str, name: &str) -> PropertyDefault {
    PropertyDefault {
        set_visibility: None,
        is_final: false,
        key: ArrayKey::str(name),
        value: Value::Null,
        static_storage: None,
        name: name.to_owned(),
        declaring: class.to_owned(),
        visibility: Visibility::Public,
        initialized: true,
        initializer: None,
        ty: None,
        is_readonly: true,
        hooks: None,
    }
}

pub(super) fn check_magic_methods(
    name: &str,
    methods: &HashMap<String, Rc<Method>>,
) -> Result<(), NativeError> {
    for forbidden in [
        "__construct",
        "__destruct",
        "__clone",
        "__get",
        "__set",
        "__unset",
        "__isset",
        "__toString",
        "__debugInfo",
        "__serialize",
        "__unserialize",
        "__sleep",
        "__wakeup",
        "__set_state",
    ] {
        if methods.contains_key(&forbidden.to_ascii_lowercase()) {
            return Err(NativeError::error(format!(
                "Enum {name} cannot include magic method {forbidden}"
            )));
        }
    }
    Ok(())
}
