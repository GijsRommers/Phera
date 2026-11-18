//! `unserialize()` allocates objects without running their constructors.

/// A linked class examined before the serialized object body is read.
pub struct SerializationClass {
    pub name: String,
    pub forbidden: bool,
    pub custom: bool,
    pub has_unserialize: bool,
}

/// The INI values used by PHP's serialization functions, local to a request.
pub struct SerializationSettings {
    pub precision: String,
    pub depth: String,
    pub precision_digits: i32,
    pub max_depth: usize,
}

impl Default for SerializationSettings {
    fn default() -> Self {
        Self {
            precision: "-1".into(),
            depth: "4096".into(),
            precision_digits: -1,
            max_depth: 4096,
        }
    }
}

pub const INCOMPLETE_CLASS: &str = "__PHP_Incomplete_Class";
pub const INCOMPLETE_CLASS_NAME: &str = "__PHP_Incomplete_Class_Name";
