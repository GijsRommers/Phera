//! Signatures for `class_exists($class, autoload: false)` and related class queries.

use phera_extension_api::{Param, ParamType};

/// Class functions are resolved once alongside the other request intrinsics.
#[derive(Clone, Copy)]
pub(crate) enum ClassFunction {
    GetClass,
    GetParentClass,
    GetCalledClass,
    GetObjectVars,
    GetClassVars,
    PropertyExists,
    MethodExists,
    ClassExists,
    InterfaceExists,
    TraitExists,
    EnumExists,
    ClassAlias,
    IsA,
    IsSubclassOf,
    ObjectId,
    ObjectHash,
    ClassMethods,
    ClassParents,
    ClassImplements,
    ClassUses,
    DeclaredClasses,
    DeclaredInterfaces,
    DeclaredTraits,
}

impl ClassFunction {
    pub(crate) const ALL: &[Self] = &[
        Self::GetClass,
        Self::GetParentClass,
        Self::GetCalledClass,
        Self::GetObjectVars,
        Self::GetClassVars,
        Self::PropertyExists,
        Self::MethodExists,
        Self::ClassExists,
        Self::InterfaceExists,
        Self::TraitExists,
        Self::EnumExists,
        Self::ClassAlias,
        Self::IsA,
        Self::IsSubclassOf,
        Self::ObjectId,
        Self::ObjectHash,
        Self::ClassMethods,
        Self::ClassParents,
        Self::ClassImplements,
        Self::ClassUses,
        Self::DeclaredClasses,
        Self::DeclaredInterfaces,
        Self::DeclaredTraits,
    ];

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::GetClass => "get_class",
            Self::GetParentClass => "get_parent_class",
            Self::GetCalledClass => "get_called_class",
            Self::GetObjectVars => "get_object_vars",
            Self::GetClassVars => "get_class_vars",
            Self::PropertyExists => "property_exists",
            Self::MethodExists => "method_exists",
            Self::ClassExists => "class_exists",
            Self::InterfaceExists => "interface_exists",
            Self::TraitExists => "trait_exists",
            Self::EnumExists => "enum_exists",
            Self::ClassAlias => "class_alias",
            Self::IsA => "is_a",
            Self::IsSubclassOf => "is_subclass_of",
            Self::ObjectId => "spl_object_id",
            Self::ObjectHash => "spl_object_hash",
            Self::ClassMethods => "get_class_methods",
            Self::ClassParents => "class_parents",
            Self::ClassImplements => "class_implements",
            Self::ClassUses => "class_uses",
            Self::DeclaredClasses => "get_declared_classes",
            Self::DeclaredInterfaces => "get_declared_interfaces",
            Self::DeclaredTraits => "get_declared_traits",
        }
    }

    pub(crate) fn parameters(self) -> &'static [Param] {
        const AUTOLOAD: Param = Param::new("autoload", ParamType::Bool).optional();
        const CLASS: Param = Param::new("class", ParamType::String);
        const OBJECT: Param = Param::new("object", ParamType::Object);
        const OBJECT_OR_CLASS: Param = Param::new("object_or_class", ParamType::Mixed);
        match self {
            Self::GetClass => const { &[OBJECT.optional()] },
            Self::GetObjectVars | Self::ObjectId | Self::ObjectHash => const { &[OBJECT] },
            Self::GetParentClass => const { &[OBJECT_OR_CLASS.optional()] },
            Self::ClassMethods => const { &[OBJECT_OR_CLASS] },
            Self::GetClassVars => const { &[CLASS] },
            Self::ClassExists => const { &[CLASS, AUTOLOAD] },
            Self::InterfaceExists => {
                const { &[Param::new("interface", ParamType::String), AUTOLOAD] }
            }
            Self::TraitExists => const { &[Param::new("trait", ParamType::String), AUTOLOAD] },
            Self::EnumExists => const { &[Param::new("enum", ParamType::String), AUTOLOAD] },
            Self::ClassAlias => {
                const { &[CLASS, Param::new("alias", ParamType::String), AUTOLOAD] }
            }
            Self::ClassParents | Self::ClassImplements | Self::ClassUses => {
                const { &[OBJECT_OR_CLASS, AUTOLOAD] }
            }
            Self::PropertyExists => {
                const { &[OBJECT_OR_CLASS, Param::new("property", ParamType::String)] }
            }
            Self::MethodExists => {
                const { &[OBJECT_OR_CLASS, Param::new("method", ParamType::String)] }
            }
            Self::IsA | Self::IsSubclassOf => {
                const {
                    &[
                        OBJECT_OR_CLASS,
                        CLASS,
                        Param::new("allow_string", ParamType::Bool).optional(),
                    ]
                }
            }
            Self::GetCalledClass
            | Self::DeclaredClasses
            | Self::DeclaredInterfaces
            | Self::DeclaredTraits => &[],
        }
    }
}
