//! Rules consume proven facts: `echo $missing;` supplies an undefined-variable fact.
//! Adding a rule requires a small implementation and one entry in `all()`.

mod abstract_new;
mod argument_count;
mod magic_constant;
mod missing_method;
mod missing_return;
mod named_argument;
mod possible_variable;
mod static_call;
mod this_context;
mod undefined_variable;
mod unknown_class;
mod unknown_constant;
mod unknown_function;
mod unknown_method;
mod unknown_property;
mod void_return;

pub enum Finding {
    UnknownClass(String),
    UnknownFunction(String),
    UnknownMethod(String, String),
    UnknownProperty(String, String),
    UnknownConstant(String),
    ArgumentCount(String),
    NamedArgument(String, String),
    UndefinedVariable(String),
    PossibleVariable(String),
    ThisContext,
    AbstractNew(String),
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
        &unknown_method::UnknownMethod,
        &unknown_property::UnknownProperty,
        &unknown_constant::UnknownConstant,
        &argument_count::ArgumentCount,
        &named_argument::NamedArgument,
        &undefined_variable::UndefinedVariable,
        &possible_variable::PossibleVariable,
        &this_context::ThisContext,
        &abstract_new::AbstractNew,
        &static_call::StaticCall,
        &missing_method::MissingMethod,
        &magic_constant::MagicConstant,
        &void_return::VoidReturn,
        &missing_return::MissingReturn,
    ]
}
