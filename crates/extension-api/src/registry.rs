//! `extension_loaded('Core')` reflects PHP modules registered for this runtime.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};

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
#[derive(Clone, Default)]
pub struct Registry {
    entries: Registration,
    deferred: Option<Arc<DeferredRegistration>>,
}

struct DeferredRegistration {
    entries: OnceLock<Registration>,
    load: fn(&mut Registry),
}

impl DeferredRegistration {
    fn load(&self) -> &Registration {
        self.entries.get_or_init(|| {
            let mut registry = Registry::new();
            (self.load)(&mut registry);
            registry.entries
        })
    }
}

#[derive(Clone, Default)]
struct Registration {
    /// Keyed by lowercase name: PHP function names are case-insensitive.
    functions: HashMap<String, FunctionEntry>,
    /// Only calls already holding a first-argument object need to check this table.
    object_handlers: HashMap<&'static str, crate::AsyncFn>,
    sync_selectors: HashMap<&'static str, crate::SyncSelector>,
    extra_named_handlers: HashMap<&'static str, crate::SyncFn>,
    trace_redacted_argument_errors: HashSet<&'static str>,
    callback_frames: HashSet<&'static str>,
    function_modules: HashMap<&'static str, &'static str>,
    function_return_types: HashMap<&'static str, &'static str>,
    function_order: Vec<&'static str>,
    ini: crate::ini::Catalog,
    extensions: Vec<&'static str>,
    /// Modules whose version is not PHP's, by lowercase name.
    module_versions: HashMap<String, &'static str>,
    bundles: Vec<&'static str>,
    constants: HashMap<String, NativeConstant>,
    constant_order: Vec<String>,
    registering_module: Option<&'static str>,
    /// In registration order, which puts every parent before its children.
    classes: Vec<ClassEntry>,
    class_modules: HashMap<&'static str, &'static str>,
}

#[derive(Clone)]
struct NativeConstant {
    value: ConstantValue,
    module: &'static str,
    deprecated: Option<&'static str>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// An embedding can publish built-ins without registering them until first use.
    pub fn deferred(load: fn(&mut Self)) -> Self {
        Self {
            deferred: Some(Arc::new(DeferredRegistration {
                entries: OnceLock::new(),
                load,
            })),
            ..Self::default()
        }
    }

    fn entries(&self) -> &Registration {
        match &self.deferred {
            Some(deferred) => deferred.load(),
            None => &self.entries,
        }
    }

    fn entries_mut(&mut self) -> &mut Registration {
        if let Some(deferred) = self.deferred.take() {
            self.entries = deferred.load().clone();
        }
        &mut self.entries
    }

    /// Runs the extension's registration against this registry.
    pub fn load(&mut self, extension: &dyn crate::Extension) {
        let previous = std::mem::replace(
            &mut self.entries_mut().registering_module,
            extension.php_module(),
        );
        extension.register(self);
        self.entries_mut().registering_module = previous;
        self.entries_mut().bundles.push(extension.name());
        if let Some(module) = extension.php_module() {
            self.php_module(module);
            if let Some(version) = extension.module_version() {
                self.entries_mut()
                    .module_versions
                    .insert(module.to_ascii_lowercase(), version);
            }
        }
    }

    /// The version a module reports for itself, when it is not PHP's.
    pub fn module_version(&self, name: &str) -> Option<&'static str> {
        let entries = self.entries();
        entries
            .module_versions
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    /// Engine-provided PHP modules can register without an extension crate.
    pub fn php_module(&mut self, name: &'static str) {
        let entries = self.entries_mut();
        if entries
            .extensions
            .iter()
            .any(|module| module.eq_ignore_ascii_case(name))
        {
            return;
        }
        entries.ini.register(name);
        entries.extensions.push(name);
    }

    /// Register SAPI settings without reporting them as a loaded extension.
    pub fn sapi_ini(&mut self, name: &str) {
        let entries = self.entries_mut();
        entries.ini.register(name);
    }

    /// Names and defaults for INI settings of modules loaded by this runtime.
    pub fn ini_entries(
        &self,
    ) -> impl Iterator<Item = (usize, &'static crate::ini::IniDefinition)> + '_ {
        let entries = self.entries();
        entries.ini.entries()
    }

    pub fn ini_entry(&self, name: &[u8]) -> Option<(usize, &'static crate::ini::IniDefinition)> {
        let entries = self.entries();
        entries.ini.get(name)
    }

    /// Register the effect of a setting already described by the PHP catalog.
    pub fn ini_update(&mut self, name: &'static str, update: crate::ini::IniUpdate) {
        let entries = self.entries_mut();
        entries.ini.update(name, update);
    }

    pub fn ini_hook(&self, name: &str) -> Option<crate::ini::IniUpdate> {
        let entries = self.entries();
        entries.ini.hook(name)
    }

    pub fn extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        let entries = self.entries();
        entries.extensions.iter().copied()
    }

    pub fn bundles(&self) -> impl Iterator<Item = &'static str> + '_ {
        let entries = self.entries();
        entries.bundles.iter().copied()
    }

    /// Panics on a duplicate name: extensions are linked at compile time, so a
    /// clash is a build bug, not something to recover from at runtime.
    pub fn function(&mut self, entry: FunctionEntry) {
        let entries = self.entries_mut();
        let key = entry.name.to_ascii_lowercase();
        assert!(
            !entries.functions.contains_key(&key),
            "function {}() is already registered",
            entry.name
        );
        if let Some(module) = entries.registering_module {
            entries.function_modules.insert(entry.name, module);
        }
        entries.function_order.push(entry.name);
        entries.functions.insert(key, entry);
    }

    /// Panics on a duplicate name, like [`Registry::function`].
    pub fn constant(&mut self, name: &str, value: impl Into<ConstantValue>) {
        self.register_constant(name, value.into(), None);
    }

    /// A retained PHP constant reports its stub deprecation only when read.
    pub fn deprecated_constant(&mut self, name: &str, value: impl Into<ConstantValue>) {
        use phera_source::deprecations::{ItemKind, kept_constant_notice};
        self.register_constant(
            name,
            value.into(),
            kept_constant_notice(ItemKind::Constant, name),
        );
    }

    fn register_constant(
        &mut self,
        name: &str,
        value: ConstantValue,
        deprecated: Option<&'static str>,
    ) {
        let entries = self.entries_mut();
        assert!(
            !entries.constants.contains_key(name),
            "constant {name} is already registered"
        );
        entries.constant_order.push(name.to_owned());
        entries.constants.insert(
            name.to_owned(),
            NativeConstant {
                value,
                module: entries.registering_module.unwrap_or("Core"),
                deprecated,
            },
        );
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
        let entries = self.entries_mut();
        if let Some(module) = entries.registering_module {
            entries.class_modules.insert(entry.name, module);
        }
        entries.classes.push(entry);
    }

    /// Clear exception trace arguments when a native function rejects its parameter list.
    pub fn redact_argument_error_trace(&mut self, function: &'static str) {
        let entries = self.entries_mut();
        entries.trace_redacted_argument_errors.insert(function);
    }

    pub fn argument_error_trace_is_redacted(&self, function: &str) -> bool {
        let entries = self.entries();
        entries.trace_redacted_argument_errors.contains(function)
    }

    /// Registers a class implemented by a bundle on behalf of a PHP module.
    pub fn class_in_module(&mut self, module: &'static str, entry: ClassEntry) {
        self.class(entry);
        self.entries_mut().class_modules.insert(entry.name, module);
    }

    /// PHP extension name for a native class's canonical name.
    pub fn class_module(&self, name: &str) -> Option<&'static str> {
        let entries = self.entries();
        entries.class_modules.get(name).copied()
    }

    /// Class names ignore case, like function names.
    pub fn get_class(&self, name: &str) -> Option<&ClassEntry> {
        let entries = self.entries();
        entries
            .classes
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Every class, parents before children.
    pub fn classes(&self) -> impl Iterator<Item = &ClassEntry> {
        let entries = self.entries();
        entries.classes.iter()
    }

    /// PHP module owning a native function's canonical name.
    pub fn function_module(&self, name: &str) -> Option<&'static str> {
        let entries = self.entries();
        entries.function_modules.get(name).copied()
    }

    /// Describe the return type and PHP ownership without growing native call entries.
    pub fn describe_function(
        &mut self,
        name: &'static str,
        return_type: &'static str,
        module: Option<&'static str>,
    ) {
        let entries = self.entries_mut();
        entries.function_return_types.insert(name, return_type);
        if let Some(module) = module {
            entries.function_modules.insert(name, module);
        }
    }

    pub fn function_return_type(&self, name: &str) -> Option<&'static str> {
        let entries = self.entries();
        entries.function_return_types.get(name).copied()
    }

    pub fn get_function(&self, name: &str) -> Option<&FunctionEntry> {
        let entries = self.entries();
        entries.functions.get(&name.to_ascii_lowercase())
    }

    pub fn get_constant(&self, name: &str) -> Option<&ConstantValue> {
        let entries = self.entries();
        entries.constants.get(name).map(|constant| &constant.value)
    }

    /// `constant('PHP_VERSION')` may be folded. A deprecated constant still
    /// needs a runtime access so it can raise its diagnostic.
    pub fn persistent_constant(&self, name: &str) -> Option<ConstantValue> {
        let entries = self.entries();
        let constant = entries.constants.get(name)?;
        constant.deprecated.is_none().then_some(constant.value)
    }

    /// Keep a synchronous implementation for other arguments, such as count($array).
    pub fn function_with_object_handler(&mut self, entry: FunctionEntry, callback: crate::AsyncFn) {
        self.function(entry);
        self.entries_mut()
            .object_handlers
            .insert(entry.name, callback);
    }

    /// The first argument is already bound and checked when the VM selects this handler.
    pub fn object_handler(&self, name: &str) -> Option<crate::AsyncFn> {
        let entries = self.entries();
        entries.object_handlers.get(name).copied()
    }

    /// An async function can select a synchronous handler for an exact shape.
    pub fn register_sync_selector(&mut self, name: &'static str, selector: crate::SyncSelector) {
        let entries = self.entries_mut();
        entries.sync_selectors.insert(name, selector);
    }

    pub fn sync_selector(&self, name: &str) -> Option<crate::SyncSelector> {
        let entries = self.entries();
        entries.sync_selectors.get(name).copied()
    }

    /// Provide an alternate body when a native variadic call supplies extra names.
    pub fn function_with_extra_named_handler(
        &mut self,
        entry: FunctionEntry,
        handler: crate::SyncFn,
    ) {
        self.function(entry);
        self.entries_mut()
            .extra_named_handlers
            .insert(entry.name, handler);
    }

    pub fn extra_named_handler(&self, name: &str) -> Option<crate::SyncFn> {
        let entries = self.entries();
        entries.extra_named_handlers.get(name).copied()
    }

    /// Register an async function whose PHP callbacks run through an internal stack frame.
    pub fn function_with_callback_frame(
        &mut self,
        name: &'static str,
        params: &'static [crate::Param],
        handler: crate::AsyncFn,
    ) {
        self.function_with_native_frame(FunctionEntry::asynchronous(name, params, handler));
    }

    /// Keep this function in PHP exception traces when its async body throws or calls PHP.
    pub fn function_with_native_frame(&mut self, entry: FunctionEntry) {
        self.function(entry);
        self.entries_mut().callback_frames.insert(entry.name);
    }

    pub fn has_callback_frame(&self, name: &str) -> bool {
        let entries = self.entries();
        entries.callback_frames.contains(name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionEntry> {
        let entries = self.entries();
        entries
            .function_order
            .iter()
            .filter_map(|name| self.get_function(name))
    }

    pub fn constants(&self) -> impl Iterator<Item = (&str, &ConstantValue)> {
        let entries = self.entries();
        entries
            .constants
            .iter()
            .map(|(name, constant)| (name.as_str(), &constant.value))
    }

    pub fn constant_names(&self) -> impl Iterator<Item = (&str, &'static str)> {
        let entries = self.entries();
        entries.constant_order.iter().filter_map(|name| {
            entries
                .constants
                .get(name)
                .map(|constant| (name.as_str(), constant.module))
        })
    }

    pub fn constant_entries(
        &self,
    ) -> impl Iterator<Item = (&str, &ConstantValue, Option<&'static str>)> {
        let entries = self.entries();
        entries
            .constants
            .iter()
            .map(|(name, constant)| (name.as_str(), &constant.value, constant.deprecated))
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
    fn deferred_registration_is_shared_until_a_clone_is_modified() {
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

        static REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);
        let registry = Registry::deferred(|registry| {
            REGISTRATIONS.fetch_add(1, Relaxed);
            registry.load(&Demo);
        });
        let mut modified = registry.clone();

        assert_eq!(REGISTRATIONS.load(Relaxed), 0);
        assert!(registry.get_function("DEMO_ANSWER").is_some());
        modified.constant("EXTRA", 7);
        assert_eq!(REGISTRATIONS.load(Relaxed), 1);
        assert_eq!(modified.get_constant("EXTRA"), Some(&ConstantValue::Int(7)));
        assert_eq!(registry.get_constant("EXTRA"), None);
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn rejects_duplicate_functions() {
        let mut registry = Registry::new();
        registry.load(&Demo);
        registry.load(&Demo);
    }
}
