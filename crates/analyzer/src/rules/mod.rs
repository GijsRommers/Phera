//! Rules consume proven facts: `echo $missing;` supplies an undefined-variable fact.
//! Adding a rule requires a small implementation and one entry in `all()`.

mod abstract_new;
mod argument_count;
mod interface_new;
mod known_named_argument;
mod magic_constant;
mod missing_method;
mod missing_return;
mod named_argument;
mod no_parent;
mod not_supported;
mod possible_variable;
mod static_call;
mod this_context;
mod undefined_variable;
mod unknown_class;
mod unknown_class_constant;
mod unknown_constant;
mod unknown_function;
mod unknown_method;
mod unknown_property;
mod unknown_static_property;
mod void_return;

pub enum Finding {
    UnknownClass(String),
    UnknownFunction(String),
    NotSupported(&'static phera_source::deprecations::Deprecation),
    UnknownMethod(String, String),
    UnknownProperty(String, String),
    UnknownConstant(String),
    ArgumentCount(String),
    NamedArgument(String, String),
    KnownNamedArgument(String, String),
    UndefinedVariable(String),
    PossibleVariable(String),
    ThisContext,
    AbstractNew(String),
    NoParent(String),
    InterfaceNew(String),
    UnknownStaticProperty(String, String),
    UnknownClassConstant(String),
    StaticCall(String, String),
    MissingMethod(String, String),
    MagicConstant(String),
    VoidReturn,
    MissingReturn(String),
}

pub trait Rule: Sync {
    fn identifier(&self) -> &'static str;
    fn level(&self) -> u8;
    fn description(&self) -> &'static str;
    fn message(&self, finding: &Finding) -> Option<String>;
}

pub fn all() -> Vec<&'static dyn Rule> {
    vec![
        &unknown_class::UnknownClass,
        &unknown_function::UnknownFunction,
        &not_supported::NotSupported,
        &unknown_method::UnknownMethod,
        &unknown_property::UnknownProperty,
        &unknown_constant::UnknownConstant,
        &argument_count::ArgumentCount,
        &named_argument::NamedArgument,
        &known_named_argument::KnownNamedArgument,
        &undefined_variable::UndefinedVariable,
        &possible_variable::PossibleVariable,
        &this_context::ThisContext,
        &abstract_new::AbstractNew,
        &no_parent::NoParent,
        &interface_new::InterfaceNew,
        &unknown_static_property::UnknownStaticProperty,
        &unknown_class_constant::UnknownClassConstant,
        &static_call::StaticCall,
        &missing_method::MissingMethod,
        &magic_constant::MagicConstant,
        &void_return::VoidReturn,
        &missing_return::MissingReturn,
    ]
}
