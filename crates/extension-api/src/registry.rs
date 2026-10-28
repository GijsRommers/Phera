use std::collections::HashMap;

use crate::{CallError, Value};

pub type NativeFunction = fn(&[Value]) -> Result<Value, CallError>;

/// Collects what extensions provide so the runtime can expose it to PHP code.
#[derive(Debug, Default)]
pub struct Registry {
    /// Keyed by lowercase name: PHP function names are case-insensitive.
    functions: HashMap<String, NativeFunction>,
    constants: HashMap<String, Value>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the extension's registration against this registry.
    pub fn load(&mut self, extension: &dyn crate::Extension) {
        extension.register(self);
    }

    /// Panics on a duplicate name: extensions are linked at compile time, so a
    /// clash is a build bug, not something to recover from at runtime.
    pub fn function(&mut self, name: &str, function: NativeFunction) {
        let key = name.to_ascii_lowercase();
        assert!(
            !self.functions.contains_key(&key),
            "function {name}() is already registered"
        );
        self.functions.insert(key, function);
    }

    /// Panics on a duplicate name, like [`Registry::function`].
    pub fn constant(&mut self, name: &str, value: impl Into<Value>) {
        assert!(
            !self.constants.contains_key(name),
            "constant {name} is already registered"
        );
        self.constants.insert(name.to_owned(), value.into());
    }

    pub fn get_function(&self, name: &str) -> Option<NativeFunction> {
        self.functions.get(&name.to_ascii_lowercase()).copied()
    }

    pub fn get_constant(&self, name: &str) -> Option<&Value> {
        self.constants.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extension;

    struct Demo;

    impl Extension for Demo {
        fn name(&self) -> &'static str {
            "demo"
        }

        fn register(&self, registry: &mut Registry) {
            registry.function("demo_answer", |_| Ok(Value::Int(42)));
            registry.constant("DEMO_VERSION", "1.0");
        }
    }

    #[test]
    fn loads_extension_functions_and_constants() {
        let mut registry = Registry::new();
        registry.load(&Demo);

        let answer = registry.get_function("DEMO_Answer").unwrap();
        assert_eq!(answer(&[]), Ok(Value::Int(42)));
        assert_eq!(
            registry.get_constant("DEMO_VERSION"),
            Some(&Value::from("1.0"))
        );
        assert_eq!(registry.get_constant("demo_version"), None);
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn rejects_duplicate_functions() {
        let mut registry = Registry::new();
        registry.load(&Demo);
        registry.load(&Demo);
    }
}
