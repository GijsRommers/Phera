//! `extension_loaded('Core')` reflects PHP modules registered for this runtime.

use std::collections::{HashMap, HashSet};

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
            ConstantValue::String(value) => Value::interned(value.as_bytes()),
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
    iconv_converter: Option<crate::IconvConverter>,
    /// Every native function, in registration order; a function's
    /// [`NativeId`](crate::NativeId) is its index here.
    functions: Vec<FunctionEntry>,
    /// The index of each function by lowercase name: PHP function names are
    /// case-insensitive. Used when a name is linked, not when a call runs.
    function_ids: HashMap<String, crate::NativeId>,
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
    /// How a fork of a snapshot gets each extension's request state.
    state_forks: HashMap<std::any::TypeId, crate::state::StateFork>,
    /// How a fork copies an open resource, by the resource's kind.
    resource_forks: HashMap<&'static str, ResourceFork>,
    resource_releases: HashMap<&'static str, ResourceReleaseFn>,
    /// Classes Phera ships written in PHP, by lowercase name: the file name
    /// their code reports and its source.
    bundled_classes: HashMap<String, BundledSource>,
}

/// PHP source that Phera ships for a class in its own namespace.
#[derive(Clone, Copy)]
pub struct BundledSource {
    /// The file name the code reports (`__FILE__`, traces).
    pub file: &'static str,
    pub source: &'static str,
}

/// Copies the data of an open resource of one kind for a fork of a
/// snapshot, or says why it cannot be copied (an open file, a socket).
pub type ResourceFork =
    fn(&dyn std::any::Any, &mut phera_value::Fork<'_>) -> Result<Box<dyn std::any::Any>, String>;

pub type ResourceReleaseFn = for<'a> fn(
    &'a mut dyn crate::Context,
    i64,
) -> crate::LocalFuture<'a, Result<(), crate::NativeError>>;

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

    pub fn set_iconv_converter(&mut self, converter: crate::IconvConverter) {
        self.iconv_converter = Some(converter);
    }

    pub fn iconv_converter(&self) -> Option<crate::IconvConverter> {
        self.iconv_converter
    }

    /// Says how a fork of a snapshot gets the request state of type `T`
    /// (see [`crate::RequestState::fork`]). Every state type an extension
    /// stores must be registered, or a snapshot that holds it is refused.
    pub fn fork_state<T: 'static>(&mut self, fork: crate::state::StateFork) {
        self.state_forks.insert(std::any::TypeId::of::<T>(), fork);
    }

    /// Ships a class written in PHP: when a script uses `name` and no class
    /// of that name is declared, the VM compiles `source` and runs it.
    pub fn bundled_class(&mut self, name: &str, source: BundledSource) {
        self.bundled_classes
            .insert(name.to_ascii_lowercase(), source);
    }

    /// The PHP source of a bundled class, by lowercase name.
    pub fn bundled_source(&self, lowercase_name: &str) -> Option<BundledSource> {
        self.bundled_classes.get(lowercase_name).copied()
    }

    /// Says how a fork copies an open resource of this kind (`"stream"`).
    pub fn fork_resource(&mut self, kind: &'static str, fork: ResourceFork) {
        self.resource_forks.insert(kind, fork);
    }

    /// How a fork copies a resource of this kind, if it can.
    pub fn resource_fork(&self, kind: &str) -> Option<ResourceFork> {
        self.resource_forks.get(kind).copied()
    }

    pub fn register_resource_release(&mut self, kind: &'static str, release: ResourceReleaseFn) {
        self.resource_releases.insert(kind, release);
    }

    pub fn resource_release(&self, kind: &str) -> Option<ResourceReleaseFn> {
        self.resource_releases.get(kind).copied()
    }

    /// The registered forks of request state, by type.
    pub fn state_forks(&self) -> &HashMap<std::any::TypeId, crate::state::StateFork> {
        &self.state_forks
    }

    /// Runs the extension's registration against this registry.
    pub fn load(&mut self, extension: &dyn crate::Extension) {
        let previous = std::mem::replace(&mut self.registering_module, extension.php_module());
        extension.register(self);
        self.registering_module = previous;
        self.bundles.push(extension.name());
        if let Some(module) = extension.php_module() {
            self.php_module(module);
            if let Some(version) = extension.module_version() {
                self.module_versions
                    .insert(module.to_ascii_lowercase(), version);
            }
        }
    }

    /// The version a module reports for itself, when it is not PHP's.
    pub fn module_version(&self, name: &str) -> Option<&'static str> {
        self.module_versions
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    /// Engine-provided PHP modules can register without an extension crate.
    pub fn php_module(&mut self, name: &'static str) {
        if self
            .extensions
            .iter()
            .any(|module| module.eq_ignore_ascii_case(name))
        {
            return;
        }
        self.ini.register(name);
        self.extensions.push(name);
    }

    /// Register SAPI settings without reporting them as a loaded extension.
    pub fn sapi_ini(&mut self, name: &str) {
        self.ini.register(name);
    }

    /// Names and defaults for INI settings of modules loaded by this runtime.
    pub fn ini_entries(
        &self,
    ) -> impl Iterator<Item = (usize, &'static crate::ini::IniDefinition)> + '_ {
        self.ini.entries()
    }

    pub fn ini_entry(&self, name: &[u8]) -> Option<(usize, &'static crate::ini::IniDefinition)> {
        self.ini.get(name)
    }

    /// Register the effect of a setting already described by the PHP catalog.
    pub fn ini_update(&mut self, name: &'static str, update: crate::ini::IniUpdate) {
        self.ini.update(name, update);
    }

    pub fn ini_hook(&self, name: &str) -> Option<crate::ini::IniUpdate> {
        self.ini.hook(name)
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
            !self.function_ids.contains_key(&key),
            "function {}() is already registered",
            entry.name
        );
        if let Some(module) = self.registering_module {
            self.function_modules.insert(entry.name, module);
        }
        self.function_order.push(entry.name);
        let id = crate::NativeId(self.functions.len() as u32);
        self.function_ids.insert(key, id);
        self.functions.push(FunctionEntry { id, ..entry });
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
        assert!(
            !self.constants.contains_key(name),
            "constant {name} is already registered"
        );
        self.constant_order.push(name.to_owned());
        self.constants.insert(
            name.to_owned(),
            NativeConstant {
                value,
                module: self.registering_module.unwrap_or("Core"),
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
        if let Some(module) = self.registering_module {
            self.class_modules.insert(entry.name, module);
        }
        self.classes.push(entry);
    }

    /// Clear exception trace arguments when a native function rejects its parameter list.
    pub fn redact_argument_error_trace(&mut self, function: &'static str) {
        self.trace_redacted_argument_errors.insert(function);
    }

    pub fn argument_error_trace_is_redacted(&self, function: &str) -> bool {
        self.trace_redacted_argument_errors.contains(function)
    }

    /// Registers a class implemented by a bundle on behalf of a PHP module.
    pub fn class_in_module(&mut self, module: &'static str, entry: ClassEntry) {
        self.class(entry);
        self.class_modules.insert(entry.name, module);
    }

    /// PHP extension name for a native class's canonical name.
    pub fn class_module(&self, name: &str) -> Option<&'static str> {
        self.class_modules.get(name).copied()
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

    /// PHP module owning a native function's canonical name.
    pub fn function_module(&self, name: &str) -> Option<&'static str> {
        self.function_modules.get(name).copied()
    }

    /// Describe the return type and PHP ownership without growing native call entries.
    pub fn describe_function(
        &mut self,
        name: &'static str,
        return_type: &'static str,
        module: Option<&'static str>,
    ) {
        self.function_return_types.insert(name, return_type);
        if let Some(module) = module {
            self.function_modules.insert(name, module);
        }
    }

    pub fn function_return_type(&self, name: &str) -> Option<&'static str> {
        self.function_return_types.get(name).copied()
    }

    pub fn get_function(&self, name: &str) -> Option<&FunctionEntry> {
        self.get_folded_function(&folded_name(name))
    }

    /// [`Registry::get_function`] for a name already in lowercase.
    pub fn get_folded_function(&self, lowercase_name: &str) -> Option<&FunctionEntry> {
        let id = self.function_ids.get(lowercase_name)?;
        self.function_at(*id)
    }

    /// The function with this number.
    #[inline]
    pub fn function_at(&self, id: crate::NativeId) -> Option<&FunctionEntry> {
        self.functions.get(id.0 as usize)
    }

    pub fn get_constant(&self, name: &str) -> Option<&ConstantValue> {
        self.constants.get(name).map(|constant| &constant.value)
    }

    /// `constant('PHP_VERSION')` may be folded. A deprecated constant still
    /// needs a runtime access so it can raise its diagnostic.
    pub fn persistent_constant(&self, name: &str) -> Option<ConstantValue> {
        let constant = self.constants.get(name)?;
        constant.deprecated.is_none().then_some(constant.value)
    }

    /// Keep a synchronous implementation for other arguments, such as count($array).
    pub fn function_with_object_handler(&mut self, entry: FunctionEntry, callback: crate::AsyncFn) {
        self.function(entry);
        self.object_handlers.insert(entry.name, callback);
    }

    /// The first argument is already bound and checked when the VM selects this handler.
    pub fn object_handler(&self, name: &str) -> Option<crate::AsyncFn> {
        self.object_handlers.get(name).copied()
    }

    /// An async function can select a synchronous handler for an exact shape.
    pub fn register_sync_selector(&mut self, name: &'static str, selector: crate::SyncSelector) {
        self.sync_selectors.insert(name, selector);
    }

    pub fn sync_selector(&self, name: &str) -> Option<crate::SyncSelector> {
        self.sync_selectors.get(name).copied()
    }

    /// Provide an alternate body when a native variadic call supplies extra names.
    pub fn function_with_extra_named_handler(
        &mut self,
        entry: FunctionEntry,
        handler: crate::SyncFn,
    ) {
        self.function(entry);
        self.register_extra_named_handler(entry.name, handler);
    }

    /// Attach the extra-name handler independently of the function's frame policy.
    pub fn register_extra_named_handler(&mut self, name: &'static str, handler: crate::SyncFn) {
        self.extra_named_handlers.insert(name, handler);
    }

    pub fn extra_named_handler(&self, name: &str) -> Option<crate::SyncFn> {
        self.extra_named_handlers.get(name).copied()
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
        self.callback_frames.insert(entry.name);
    }

    pub fn has_callback_frame(&self, name: &str) -> bool {
        self.callback_frames.contains(name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionEntry> {
        self.function_order
            .iter()
            .filter_map(|name| self.get_function(name))
    }

    pub fn constants(&self) -> impl Iterator<Item = (&str, &ConstantValue)> {
        self.constants
            .iter()
            .map(|(name, constant)| (name.as_str(), &constant.value))
    }

    pub fn constant_names(&self) -> impl Iterator<Item = (&str, &'static str)> {
        self.constant_order.iter().filter_map(|name| {
            self.constants
                .get(name)
                .map(|constant| (name.as_str(), constant.module))
        })
    }

    pub fn constant_entries(
        &self,
    ) -> impl Iterator<Item = (&str, &ConstantValue, Option<&'static str>)> {
        self.constants
            .iter()
            .map(|(name, constant)| (name.as_str(), &constant.value, constant.deprecated))
    }
}

/// A PHP function or class name in lowercase, the form name lookups use
/// (PHP's names are case-insensitive). Most names are written in
/// lowercase already, and are given back as they are, without a copy.
pub fn folded_name(name: &str) -> std::borrow::Cow<'_, str> {
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(name.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(name)
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
