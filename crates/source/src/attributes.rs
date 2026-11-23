//! PHP's built-in attribute targets, such as `#[SensitiveParameter]` on a parameter.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AttributeTarget {
    Class = 1,
    Function = 2,
    Method = 4,
    Property = 8,
    ClassConstant = 16,
    Parameter = 32,
    Constant = 64,
}

impl AttributeTarget {
    pub fn name(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Function => "function",
            Self::Method => "method",
            Self::Property => "property",
            Self::ClassConstant => "class constant",
            Self::Parameter => "parameter",
            Self::Constant => "constant",
        }
    }
}

pub const BUILTIN_TARGETS: &[(&str, i64)] = &[
    ("Attribute", 1),
    ("AllowDynamicProperties", 1),
    ("Deprecated", 87),
    ("ReturnTypeWillChange", 4),
    ("Override", 12),
    ("SensitiveParameter", 32),
    ("NoDiscard", 6),
    ("DelayedTargetValidation", 127),
];

pub fn builtin_flags(name: &str) -> Option<i64> {
    BUILTIN_TARGETS
        .iter()
        .find(|(attribute, _)| attribute.eq_ignore_ascii_case(name))
        .map(|(_, mask)| *mask)
}

pub fn target_names(flags: i64) -> String {
    use AttributeTarget::*;
    [
        Class,
        Function,
        Method,
        Property,
        ClassConstant,
        Parameter,
        Constant,
    ]
    .into_iter()
    .filter(|target| flags & (*target as i64) != 0)
    .map(AttributeTarget::name)
    .collect::<Vec<_>>()
    .join(", ")
}
