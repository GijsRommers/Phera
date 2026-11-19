//! `unserialize()` allocates objects without running their constructors.

use std::{collections::HashSet, rc::Rc};

use crate::ArrayKey;
use crate::value::ClassHandle;

/// The decoder's class allowlist and depth at entry into a native C: payload.
#[derive(Clone, Default)]
pub struct UnserializePolicy {
    allowed: Option<Rc<HashSet<Vec<u8>>>>,
    pub max_depth: usize,
    pub depth: usize,
}

impl UnserializePolicy {
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            max_depth,
            ..Self::default()
        }
    }

    pub fn from_ini(context: &mut dyn crate::Context) -> Self {
        Self::with_max_depth(
            context
                .ini_get(b"unserialize_max_depth")
                .map_or(4096, |setting| {
                    usize::try_from(crate::ini::parse_quantity(setting.as_bytes()).0).unwrap_or(0)
                }),
        )
    }

    pub fn allow_every_class(&mut self) {
        self.allowed = None;
    }

    pub fn restrict_to(&mut self, classes: HashSet<Vec<u8>>) {
        self.allowed = Some(Rc::new(classes));
    }

    pub fn allows(&self, name: &[u8]) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|list| list.contains(&name.to_ascii_lowercase()))
    }

    pub fn depth_exceeded(&self, additional: usize) -> bool {
        self.max_depth != 0 && self.depth.saturating_add(additional) >= self.max_depth
    }

    pub fn depth_warning(&self, function: &str) -> String {
        format!(
            "{function}(): Maximum depth of {} exceeded. The depth limit can be changed \
             using the max_depth unserialize() option or the unserialize_max_depth ini setting",
            self.max_depth
        )
    }
}

/// PHP shares its decoder policy while running a C: deserializer. A
/// Serializable implementation can call another decoder from its method.
#[derive(Default)]
pub struct ActiveUnserializePolicy(pub Option<UnserializePolicy>);

impl ActiveUnserializePolicy {
    pub fn current(context: &mut dyn crate::Context) -> UnserializePolicy {
        context
            .state()
            .get::<Self>()
            .and_then(|active| active.0.clone())
            .unwrap_or_else(|| UnserializePolicy::from_ini(context))
    }

    pub fn replace(
        context: &mut dyn crate::Context,
        policy: Option<UnserializePolicy>,
    ) -> Option<UnserializePolicy> {
        let previous = context.state().take::<Self>().and_then(|active| active.0);
        if let Some(policy) = policy {
            context.state().get_or_default::<Self>().0 = Some(policy);
        }
        previous
    }
}

/// A linked class examined before the serialized object body is read.
pub struct SerializationClass {
    pub name: String,
    pub forbidden: bool,
    pub custom: bool,
    pub has_unserialize: bool,
    pub plain: Option<PlainSerializedClass>,
}

/// A class whose scalar property table can be filled before publishing the object.
pub struct PlainSerializedClass {
    pub class: Rc<dyn ClassHandle>,
    pub fields: Rc<[SerializedProperty]>,
}

/// A linked property's canonical storage key and its serialized spelling.
pub struct SerializedProperty {
    pub key: ArrayKey,
    pub token: Vec<u8>,
}

pub const INCOMPLETE_CLASS: &str = "__PHP_Incomplete_Class";
pub const INCOMPLETE_CLASS_NAME: &str = "__PHP_Incomplete_Class_Name";

/// All property and method handlers use the original class's name in this diagnostic.
pub fn incomplete_object_message(object: &crate::Object, action: &str) -> String {
    let name = match object.get(&crate::ArrayKey::str(INCOMPLETE_CLASS_NAME)) {
        Some(crate::Value::String(name)) => name.to_string_lossy(),
        _ => "unknown".into(),
    };
    format!(
        "The script tried to {action} an incomplete object. Please ensure that the \
         class definition \"{name}\" of the object you are trying to operate on was loaded _before_ unserialize() \
         gets called or provide an autoloader to load the class definition"
    )
}

/// Return the current decimal precision for serialize(), var_dump(), and var_export().
pub fn precision(context: &mut dyn crate::Context) -> i32 {
    context.serialization_precision()
}

/// Resolve the class and constant in a serialized `E:` token.
pub enum EnumCaseLookup {
    MissingClass,
    NotEnum,
    MissingCase,
    NotCase,
    Found(crate::Value),
}
