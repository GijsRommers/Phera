//! `unserialize()` allocates objects without running their constructors.

use std::rc::Rc;

use crate::ArrayKey;
use crate::value::ClassHandle;

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

/// The INI values used by PHP's serialization functions, local to a request.
pub struct SerializationSettings {
    pub precision: String,
    pub depth: String,
    pub callback: String,
    pub precision_digits: i32,
    pub max_depth: usize,
}

impl Default for SerializationSettings {
    fn default() -> Self {
        Self {
            precision: "-1".into(),
            depth: "4096".into(),
            callback: String::new(),
            precision_digits: -1,
            max_depth: 4096,
        }
    }
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
    context
        .state()
        .get::<SerializationSettings>()
        .map_or(-1, |settings| settings.precision_digits)
}

/// Resolve the class and constant in a serialized `E:` token.
pub enum EnumCaseLookup {
    MissingClass,
    NotEnum,
    MissingCase,
    NotCase,
    Found(crate::Value),
}
