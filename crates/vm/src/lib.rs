//! Request-local async interpreter. PHP calls push detachable heap frames.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{Registry, RequestState};
use phera_value::{PhpArray, Value};

pub use native_tables::NativeTables;
pub use snapshot::{BootOutcome, Snapshot, SnapshotMode};

use crate::calls::frame::{Frame, FrameView};
use crate::path::AccessPath;

/// Compiler invariant failures are PHP errors. Keep literal construction here:
/// formatting a dynamic detail changed code generation enough to miss the speed gate.
macro_rules! internal_error {
    ($detail:literal) => {
        phera_extension_api::NativeError::error(concat!("Phera internal error: ", $detail))
    };
}

mod access_mode;
mod array_map;
mod array_slots;
mod arrays;
mod attribute_diagnostics;
mod autoload;
mod builtin_calls;
mod bundled_classes;
mod callable_classes;
mod callable_introspection;
mod callables;
mod callback_errors;
mod calls;
mod class_alias;
mod class_constants;
mod class_function;
mod class_intrinsics;
mod class_listings;
mod class_member_queries;
mod class_queries;
mod class_types;
mod classes;
mod closure_binding;
mod closure_drop;
mod closure_equality;
mod closure_initializers;
mod closure_methods;
#[cfg(test)]
mod closure_tests;
mod closure_types;
mod closures;
mod code_table;
mod compile_failures;
mod constant_listings;
mod constant_table;
mod constants;
mod context;
mod control_flow;
mod cycles;
mod debug_info;
mod declaration_diagnostics;
mod default_autoload;
#[cfg(test)]
mod destructor_tests;
mod destructors;
mod diagnostic_output;
mod diagnostic_reporter;
mod diagnostics;
mod dimension_instructions;
mod dimensions;
mod dispatch;
mod dynamic_variables;
mod element_writes;
mod error_handler_signatures;
mod error_handlers;
mod error_traces;
mod error_values;
mod errors;
mod eval;
mod exception_rendering;
#[cfg(test)]
mod exception_tests;
mod exception_traces;
mod exception_unwind;
mod exceptions;
mod execution_time;
mod extract;
mod failed_construction;
mod fast;
mod fast_values;
mod fibers;
mod finally;
mod function_facts;
mod function_introspection;
mod function_table;
mod generators;
mod global_instructions;
mod global_paths;
mod globals;
mod handler_installation;
mod hooked_property_iteration;
mod hooked_property_views;
mod include_lookup;
mod include_scope;
mod include_state;
mod includes;
mod ini;
mod initializer_tasks;
mod initializers;
mod inline_stack;
mod instructions;
mod integer_arithmetic;
mod internal_iterator;
mod interpreter;
mod intrinsic_calls;
mod intrinsics;

/// Whether the engine supplies a function independently of extension registration.
pub fn is_intrinsic_function(name: &str) -> bool {
    intrinsics::Intrinsic::resolve(name).is_some()
}
mod iteration;
mod iterator_instructions;
mod iterators;
mod last_error;
mod lazy_continuation;
mod lazy_defaults;
mod lazy_objects;
mod lazy_property_access;
mod locals;
mod magic_method;
mod magic_mutations;
mod magic_path_instructions;
mod magic_places;
mod magic_properties;
mod magic_property_write;
mod magic_references;
mod native_diagnostics;
mod native_property_views;
mod native_tables;
mod object_comparison;
mod object_iteration;
mod object_operators;
mod operators;
mod ops;
mod output;
mod output_buffer;
mod path;
mod path_dimensions;
mod path_instructions;
mod path_update;
mod place_mutation;
mod places;
mod properties;
mod property_hook_continuation;
mod property_hook_sites;
mod property_mutation;
mod property_set_visibility;
mod property_sites;
mod property_storage;
mod property_strings;
mod property_targets;
mod reference_holders;
mod references;
mod reflection;
mod request_callbacks;
mod request_end;
mod scope_intrinsics;
mod sensitive_parameters;
mod serialization;
mod snapshot;
mod spl_autoload;
mod static_places;
mod static_variables;
mod string_comparison;
mod string_offsets;
mod stringable;
mod traversable_foreach;
mod traversable_unpack;
mod traversal;
mod trigger_errors;
mod typed_references;
mod types;
mod unit_caches;
mod unpacking;
mod unsupported_items;
mod worker_code;

pub use include_lookup::{
    CodeLoadError, CodeLoader, IncludeCachePolicy, IncludeLoad, IncludeLookup, RelativeInclude,
    StreamInclude,
};

/// Receives each PHP output write while Vm also retains its output buffer.
pub trait OutputSink {
    fn write(&mut self, bytes: &[u8]);
    fn flush(&mut self) {}
}

impl OutputSink for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes)
    }
}

/// Default limit permits 200,000-deep PHP recursion without consuming Rust stack.
pub const DEFAULT_FRAME_LIMIT: usize = 250_000;

/// One request interpreter; PHP calls use heap frames rather than Rust recursion.
pub struct Vm {
    classes: classes::ClassTable,
    /// The class table's id pool, held here as well so that the
    /// interpreter loop can read it while it borrows the class table.
    object_ids: Rc<phera_value::ObjectIds>,
    enum_cases: classes::enums::EnumCases,
    native_callback_sites: Vec<error_traces::NativeCallbackSite>,
    debug_objects: Vec<u32>,
    property_guards: Vec<magic_properties::PropertyGuard>,
    unset_typed_properties: std::collections::HashMap<u32, magic_properties::UnsetProperties>,
    clone_scopes: classes::cloning::CloneScopes,
    closure_captures: Vec<closures::PendingCapture>,
    /// Pure defaults retain lexical class scope while running outside a method frame.
    initializer_scope: Option<String>,
    initializer_class: Option<initializers::InitializerClass>,
    argument_origin: calls::callback_binding::ArgumentOrigin,
    native_called_class: Option<std::rc::Rc<classes::Class>>,
    pending_dimension: Option<dimensions::DimensionPath>,
    returned_hook: Option<property_hook_continuation::ReturnedHook>,
    native_view_depth: Option<usize>,
    prelinked_classes: std::collections::HashSet<(usize, usize)>,
    function_table: function_table::FunctionTable,
    /// Every loaded file and function of the request, by number.
    code: code_table::CodeTable,
    /// Static properties and evaluated class constants of this request.
    class_states: classes::class_state::ClassStates,
    globals: globals::Globals,
    extension_values: std::collections::HashMap<&'static str, Value>,
    static_variables: static_variables::StaticVariables,
    diagnostic_reporter: diagnostic_reporter::DiagnosticReporter,
    diagnostic_location: Option<diagnostics::SourceLocation>,
    include_state: include_state::IncludeState,
    call_stack: calls::call_stack::CallStack,
    /// Only entered finally subroutines allocate continuation storage.
    finally_stack: Vec<finally::SavedFinally>,
    generator_suspension: Option<generators::Suspension>,
    generator_traces: Vec<generators::trace::Site>,
    generator_releases: Option<std::rc::Rc<generators::cleanup::Releases>>,
    draining_generators: bool,
    closing_generator: Option<usize>,
    /// Number of dispatched instructions, for reproducible benchmark reports.
    pub executed_instructions: u64,
    registry: Arc<Registry>,
    /// All PHP output emitted during this request.
    pub output: Vec<u8>,
    output_writer: output::Output,
    request_callbacks: request_callbacks::RequestCallbacks,
    registers: calls::registers::Registers,
    constants: constant_table::ConstantTable,
    /// PHP call depth limit, independent of the native Rust stack.
    pub frame_limit: usize,
    status: i32,
    error_handlers: error_handlers::ErrorHandlers,
    /// Whether output or a diagnostic needs to invoke a PHP callback.
    pending_callbacks: bool,
    failed_native: Option<error_traces::NativeTrace>,
    /// The arguments of a native call that missed a class, kept by the miss
    /// for its second call (see `calls::native_retries`).
    missed_native: Option<Vec<Value>>,
    /// What extensions remember during this request (response headers, ...).
    state: RequestState,
    reflection_cache: Option<Box<reflection::ReflectionCache>>,
    property_references: std::rc::Rc<std::cell::RefCell<reference_holders::ReferenceHolders>>,
    autoload: autoload::Autoloaders,
    request_end: request_end::RequestEnd,
    cycles: cycles::CycleCollector,
    ini_overrides: ini::IniOverrides,
    execution_time: Rc<execution_time::ExecutionTime>,
    active_fiber: Option<Box<fibers::Active>>,
    fiber_bailout: bool,
    suspended_fibers: fibers::SuspendedFibers,
    /// What `Phera\snapshot()` does here, and the state it kept.
    snapshot: snapshot::SnapshotState,
}

impl Vm {
    /// Create isolated request state from an immutable native registry.
    pub fn new(registry: Arc<Registry>) -> Self {
        Self::with_native_tables(&NativeTables::new(registry))
    }

    /// Register VM-owned fiber classes after Core and Reflection.
    pub fn register_fiber_classes(registry: &mut Registry) {
        fibers::register(registry);
    }

    /// Start a fresh request with the worker's already linked built-ins.
    pub fn with_native_tables(native: &NativeTables) -> Self {
        let classes = classes::ClassTable::new(native.classes.clone(), native.linked.clone());
        let object_ids = classes.ids_handle();
        Self {
            enum_cases: classes::enums::EnumCases::default(),
            object_ids,
            classes,
            request_end: Default::default(),
            cycles: Default::default(),
            autoload: Default::default(),
            ini_overrides: Default::default(),
            execution_time: Default::default(),
            active_fiber: None,
            fiber_bailout: false,
            suspended_fibers: Default::default(),
            snapshot: Default::default(),
            native_callback_sites: Vec::new(),
            debug_objects: Vec::new(),
            property_guards: Vec::new(),
            unset_typed_properties: Default::default(),
            clone_scopes: classes::cloning::CloneScopes::default(),
            property_references: Default::default(),
            closure_captures: Vec::new(),
            initializer_scope: None,
            initializer_class: None,
            argument_origin: calls::callback_binding::ArgumentOrigin::Php,
            native_called_class: None,
            pending_dimension: None,
            returned_hook: None,
            native_view_depth: None,
            prelinked_classes: std::collections::HashSet::new(),
            function_table: function_table::FunctionTable::new(),
            code: code_table::CodeTable::for_worker(&native.code),
            class_states: classes::class_state::ClassStates::for_worker(&native.slots),
            globals: globals::Globals::new(),
            extension_values: Default::default(),
            static_variables: static_variables::StaticVariables::new(),
            diagnostic_reporter: diagnostic_reporter::DiagnosticReporter::new(),
            diagnostic_location: None,
            include_state: include_state::IncludeState::new(),
            call_stack: calls::call_stack::CallStack::new(),
            finally_stack: Vec::new(),
            generator_suspension: None,
            generator_traces: Vec::new(),
            generator_releases: None,
            draining_generators: false,
            closing_generator: None,
            executed_instructions: 0,
            registry: native.registry.clone(),
            output: vec![],
            output_writer: output::Output::new(),
            request_callbacks: Default::default(),
            registers: calls::registers::Registers::new(),
            constants: constant_table::ConstantTable::new(native.constants.clone()),
            frame_limit: DEFAULT_FRAME_LIMIT,
            status: 0,
            error_handlers: error_handlers::ErrorHandlers::default(),
            pending_callbacks: false,
            failed_native: None,
            missed_native: None,
            state: RequestState::default(),
            reflection_cache: None,
        }
    }

    pub fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.set_local_loader(Rc::new(loader))
    }

    /// A worker-local loader stays on the same thread as the VM.
    pub fn set_local_loader(&mut self, loader: Rc<dyn CodeLoader>) {
        self.include_state.set_loader(loader)
    }

    /// The startup policy also governs the script that begins a web request.
    pub fn include_cache_policy(&self) -> IncludeCachePolicy {
        self.include_state.policy
    }

    pub fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.output_writer.set_sink(sink)
    }

    /// Defines a request-local constant, such as a CLI stream.
    pub fn set_constant(&mut self, name: &str, value: Value) {
        self.constants.insert(name.to_owned(), value, None, None);
    }

    /// Defines a global variable before the script starts, such as a
    /// superglobal the runtime fills from the request.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.publish_global(name.into(), value);
    }

    /// The value of a global variable (seen through a reference), such as
    /// a superglobal the script may have changed.
    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.get(name).map(|value| value.deref().clone())
    }

    /// The extensions' per-request state, for the runtime to read once the
    /// script has finished.
    pub fn state_mut(&mut self) -> &mut RequestState {
        &mut self.state
    }

    /// Expose command-line arguments as PHP argv and argc globals.
    pub fn set_args(&mut self, arguments: &[String]) {
        let mut array = PhpArray::new();
        for argument in arguments {
            let _ = array.push(Value::from(argument.clone()));
        }
        self.globals
            .insert("argv".into(), Value::Array(Rc::new(array)));
        self.globals
            .insert("argc".into(), Value::Int(arguments.len() as i64));
    }

    /// Run a compiled top-level function on the request heap stack and report its exit status.
    pub async fn run(&mut self, unit: Arc<CompiledUnit>) -> i32 {
        self.execution_time
            .clone()
            .track(self.run_request(unit))
            .await
    }

    async fn run_request(&mut self, unit: Arc<CompiledUnit>) -> i32 {
        let main = self.code.load(&unit);
        for id in &unit.hoisted {
            if let Some(function) = unit.functions.get(id.index()) {
                let index = self.code.declared(main, id.index());
                self.function_table
                    .insert(function.name.to_ascii_lowercase(), index);
            }
        }
        self.include_state.record_entry(main, unit.file.clone());
        let registers = unit.main.registers as usize;
        self.registers.grow_to(registers);
        self.call_stack.push(Frame::new(main, 0, None));
        for (index, name) in unit.main.locals.iter().enumerate() {
            if let Some(name) = unit.names.get(name.index()) {
                self.globals.declare(name);
                self.registers.window_mut(index, 1)[0] = self.globals.take(name);
            }
        }
        self.publish_declaration_warnings(&unit);
        if let Err(error) = self.hoist_classes(&unit, main) {
            self.unwind(error);
            return self.end_script(request_end::ScriptEnd::Fatal).await;
        }
        self.run_script_body().await
    }

    /// Runs the script from where its frames are to its end, then ends the
    /// request. A fork of a snapshot starts here (see `crate::snapshot`).
    async fn run_script_body(&mut self) -> i32 {
        let end = match self.interpret(0).await {
            Ok(_) => request_end::ScriptEnd::Destructible,
            // A boot run stopped at `Phera\snapshot()`: its state is kept,
            // and nothing more of it runs.
            Err(_) if self.snapshot.taken.is_some() => return self.status,
            Err(error) => {
                let mut error = self.materialize_error(error);
                // The frames the error left released their variables;
                // PHP runs those destructors before it reports the error.
                if let Err(failure) = self.run_destructors().await {
                    error = self.materialize_error(failure);
                }
                let end = request_end::ScriptEnd::of(&error);
                self.report_uncaught(error).await;
                end
            }
        };
        self.end_script(end).await
    }

    /// Output, shutdown functions, destructors and headers at the end.
    async fn end_script(&mut self, end: request_end::ScriptEnd) -> i32 {
        if self.output_pending()
            && let Err(error) = self.drain_output().await
        {
            self.unwind(error);
        }
        self.finish_request(end).await;
        self.finish_request_headers().await;
        self.finish_resource_finalizers().await;
        self.status
    }

    /// The running frame, with its code and side-table entry.
    #[inline]
    fn current(&self) -> Option<FrameView<'_>> {
        let depth = self.call_stack.depth().checked_sub(1)?;
        self.frame_at(depth)
    }

    /// The frame at `depth`, with its code and side-table entry.
    fn frame_at(&self, depth: usize) -> Option<FrameView<'_>> {
        Some(FrameView {
            frame: *self.call_stack.at(depth)?,
            code: &self.code,
            extras: self.call_stack.extras(depth),
            registers: &self.registers,
            classes: &self.classes,
        })
    }

    /// Every frame, from the script's top-level code to the running one.
    fn frames(&self) -> impl DoubleEndedIterator<Item = FrameView<'_>> + ExactSizeIterator {
        let (code, stack) = (&self.code, &self.call_stack);
        let (registers, classes) = (&self.registers, &self.classes);
        stack
            .frames()
            .iter()
            .enumerate()
            .map(move |(depth, frame)| FrameView {
                frame: *frame,
                code,
                extras: stack.extras(depth),
                registers,
                classes,
            })
    }

    /// Read an operator operand without creating another PHP value owner.
    #[inline]
    fn operand(&self, register: Reg) -> phera_value::ValueRef<'_> {
        self.current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .unwrap_or(&Value::Null)
            .deref()
    }

    #[inline]
    fn get(&self, register: Reg) -> Value {
        self.current()
            .map(|frame| frame.get(&self.registers, register))
            .unwrap_or_default()
    }

    /// A register of the running frame, for writing.
    #[inline]
    fn current_slot_mut(&mut self, register: Reg) -> Option<&mut Option<Value>> {
        let frame = self.current()?;
        let (base, count) = (frame.base(), frame.count());
        self.registers
            .window_mut(base, count)
            .get_mut(register.index())
    }

    #[inline]
    fn set(&mut self, register: Reg, value: Value) {
        if let Some(frame) = self.current() {
            let (base, count) = (frame.base(), frame.count());
            if let Some(slot) = self
                .registers
                .window_mut(base, count)
                .get_mut(register.index())
            {
                *slot = Some(value)
            }
        }
    }
}
