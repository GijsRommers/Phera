//! `extension_loaded('Core')` reflects PHP modules registered for this runtime.

use std::collections::HashMap;

use phera_value::Value;

use crate::{ClassEntry, FunctionEntry};

/// The value of a constant an extension defines. Kept apart from [`Value`] so
/// a registry holds no `Rc` and can be shared between worker threads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstantValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(&'static str),
    /// `[]`, the only array a default value needs.
    EmptyArray,
}

impl ConstantValue {
    pub fn to_value(&self) -> Value {
        match self {
            ConstantValue::Null => Value::Null,
            ConstantValue::Bool(value) => Value::Bool(*value),
            ConstantValue::Int(value) => Value::Int(*value),
            ConstantValue::Float(value) => Value::Float(*value),
            ConstantValue::String(value) => Value::from(*value),
            ConstantValue::EmptyArray => Value::from(phera_value::PhpArray::new()),
        }
    }
}

impl From<bool> for ConstantValue {
    fn from(value: bool) -> Self {
        ConstantValue::Bool(value)
    }
}

impl From<i64> for ConstantValue {
    fn from(value: i64) -> Self {
        ConstantValue::Int(value)
    }
}

impl From<f64> for ConstantValue {
    fn from(value: f64) -> Self {
        ConstantValue::Float(value)
    }
}

impl From<&'static str> for ConstantValue {
    fn from(value: &'static str) -> Self {
        ConstantValue::String(value)
    }
}

/// Collects what extensions provide so the runtime can expose it to PHP code.
/// Built once at startup and then shared, read-only, by every worker thread.
#[derive(Default)]
pub struct Registry {
    /// Keyed by lowercase name: PHP function names are case-insensitive.
    functions: HashMap<String, FunctionEntry>,
    function_order: Vec<&'static str>,
    extensions: Vec<&'static str>,
    bundles: Vec<&'static str>,
    constants: HashMap<String, ConstantValue>,
    /// In registration order, which puts every parent before its children.
    classes: Vec<ClassEntry>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs the extension's registration against this registry.
    pub fn load(&mut self, extension: &dyn crate::Extension) {
        extension.register(self);
        self.bundles.push(extension.name());
        if let Some(module) = extension.php_module() {
            self.php_module(module);
        }
    }

    /// Engine-provided PHP modules can register without an extension crate.
    pub fn php_module(&mut self, name: &'static str) {
        self.extensions.push(name);
    }

    pub fn extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.extensions.iter().copied()
    }

    pub fn bundles(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.bundles.iter().copied()
    }

    /// Panics on a duplicate name: extensions are linked at compile time, so a
    /// clash is a build bug, not something to recover from at runtime.
    pub fn function(&mut self, entry: FunctionEntry) {
        let key = entry.name.to_ascii_lowercase();
        assert!(
            !self.functions.contains_key(&key),
            "function {}() is already registered",
            entry.name
        );
        self.function_order.push(entry.name);
        self.functions.insert(key, entry);
    }

    /// Panics on a duplicate name, like [`Registry::function`].
    pub fn constant(&mut self, name: &str, value: impl Into<ConstantValue>) {
        assert!(
            !self.constants.contains_key(name),
            "constant {name} is already registered"
        );
        self.constants.insert(name.to_owned(), value.into());
    }

    /// Panics on a duplicate name or an unregistered parent or interface:
    /// both are build bugs, like a duplicate function.
    pub fn class(&mut self, entry: ClassEntry) {
        assert!(
            self.get_class(entry.name).is_none(),
            "class {} is already registered",
            entry.name
        );
        for required in entry.parent.iter().chain(entry.interfaces) {
            assert!(
                self.get_class(required).is_some(),
                "class {} needs {required}, which is not registered before it",
                entry.name
            );
        }
        self.classes.push(entry);
    }

    /// Class names ignore case, like function names.
    pub fn get_class(&self, name: &str) -> Option<&ClassEntry> {
        self.classes
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Every class, parents before children.
    pub fn classes(&self) -> impl Iterator<Item = &ClassEntry> {
        self.classes.iter()
    }

    pub fn get_function(&self, name: &str) -> Option<&FunctionEntry> {
        self.functions.get(&name.to_ascii_lowercase())
    }

    pub fn get_constant(&self, name: &str) -> Option<&ConstantValue> {
        self.constants.get(name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionEntry> {
        self.function_order
            .iter()
            .filter_map(|name| self.get_function(name))
    }

    pub fn constants(&self) -> impl Iterator<Item = (&str, &ConstantValue)> {
        self.constants
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;
    use crate::{Context, Extension, NativeFn, NativeResult, Param, ParamType};

    struct Demo;

    fn demo_answer(_: &mut dyn Context, _: &[Value]) -> NativeResult {
        Ok(Value::Int(42))
    }

    impl Extension for Demo {
        fn name(&self) -> &'static str {
            "demo"
        }

        fn register(&self, registry: &mut Registry) {
            const PARAMS: &[Param] = &[
                Param::new("a", ParamType::Int),
                Param::new("b", ParamType::Int).optional(),
            ];
            registry.function(FunctionEntry::sync("demo_answer", PARAMS, demo_answer));
            registry.constant("DEMO_VERSION", "1.0");
        }
    }

    #[test]
    fn loads_extension_functions_and_constants() {
        let mut registry = Registry::new();
        registry.load(&Demo);

        let entry = registry.get_function("DEMO_Answer").unwrap();
        assert_eq!((entry.min_args(), entry.max_args()), (1, Some(2)));
        let NativeFn::Sync(answer) = entry.handler else {
            panic!("expected a sync function");
        };
        let result = answer(&mut TestContext::default(), &[]).unwrap();
        assert_eq!(result, Value::Int(42));

        assert_eq!(
            registry.get_constant("DEMO_VERSION"),
            Some(&ConstantValue::String("1.0"))
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
