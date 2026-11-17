//! Request-local async interpreter. PHP calls push detachable heap frames.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{LocalFuture, Registry, RequestState};
use phera_value::{PhpArray, Value};

use crate::calls::frame::Frame;
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
mod callable_classes;
mod callable_introspection;
mod callables;
mod callback_errors;
mod calls;
mod class_constants;
mod class_intrinsics;
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
mod constant_table;
mod constants;
mod context;
mod control_flow;
mod declaration_diagnostics;
mod diagnostic_reporter;
mod diagnostics;
mod dispatch;
mod dynamic_variables;
mod error_handler_signatures;
mod error_handlers;
mod error_traces;
mod error_values;
mod errors;
mod exception_rendering;
#[cfg(test)]
mod exception_tests;
mod exception_traces;
mod exception_unwind;
mod exceptions;
mod extract;
mod fast;
mod finally;
mod function_table;
mod global_instructions;
mod global_paths;
mod globals;
mod handler_installation;
mod include_scope;
mod include_state;
mod includes;
mod initializers;
mod inline_stack;
mod instructions;
mod integer_arithmetic;
mod interpreter;
mod intrinsic_calls;
mod intrinsics;
mod iteration;
mod iterator_instructions;
mod iterators;
mod locals;
mod native_diagnostics;
mod native_strings;
mod object_instructions;
mod object_iteration;
mod operators;
mod ops;
mod output;
mod path;
mod path_instructions;
mod path_update;
mod place_mutation;
mod places;
mod properties;
mod property_mutation;
mod property_sites;
mod property_storage;
mod property_targets;
mod reference_holders;
mod references;
mod scalar_batches;
mod scope_intrinsics;
mod static_variables;
mod string_offsets;
mod trigger_errors;
mod typed_references;
mod types;
mod unit_caches;
mod unpacking;
mod value_instructions;

/// Resolves PHP includes through the embedding runtime code cache.
pub trait CodeLoader: Send + Sync {
    fn load(&self, path: std::path::PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>>;
}

/// Receives each PHP output write while Vm also retains its output buffer.
pub trait OutputSink {
    fn write(&mut self, bytes: &[u8]);
}

impl OutputSink for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes)
    }
}

/// Default limit permits 200,000-deep PHP recursion without consuming Rust stack.
pub const DEFAULT_FRAME_LIMIT: usize = 250_000;
/// Per-symbol resolved callees shared by frames of one compiled unit.
type SymbolCache = Rc<unit_caches::UnitCache>;

/// One request interpreter; PHP calls use heap frames rather than Rust recursion.
pub struct Vm {
    classes: classes::ClassTable,
    closure_captures: Vec<closures::PendingCapture>,
    /// Pure defaults retain lexical class scope while running outside a method frame.
    initializer_scope: Option<String>,
    argument_origin: calls::callback_binding::ArgumentOrigin,
    prelinked_classes: std::collections::HashSet<(usize, usize)>,
    function_table: function_table::FunctionTable,
    unit_caches: unit_caches::UnitCaches,
    globals: globals::Globals,
    static_variables: static_variables::StaticVariables,
    diagnostic_reporter: diagnostic_reporter::DiagnosticReporter,
    diagnostic_location: Option<diagnostics::SourceLocation>,
    include_state: include_state::IncludeState,
    call_stack: calls::call_stack::CallStack,
    /// Only entered finally subroutines allocate continuation storage.
    finally_stack: Vec<finally::SavedFinally>,
    /// Number of dispatched instructions, for reproducible benchmark reports.
    pub executed_instructions: u64,
    registry: Arc<Registry>,
    /// All PHP output emitted during this request.
    pub output: Vec<u8>,
    output_writer: output::Output,
    registers: calls::registers::Registers,
    constants: constant_table::ConstantTable,
    /// PHP call depth limit, independent of the native Rust stack.
    pub frame_limit: usize,
    status: i32,
    error_handlers: error_handlers::ErrorHandlers,
    failed_native: Option<error_traces::NativeTrace>,
    /// What extensions remember during this request (response headers, ...).
    state: RequestState,
    property_references: std::rc::Rc<std::cell::RefCell<reference_holders::ReferenceHolders>>,
}

impl Vm {
    /// Create isolated request state from an immutable native registry.
    pub fn new(registry: Arc<Registry>) -> Self {
        let constants = registry
            .constants()
            .map(|(name, value)| (name.to_owned(), value.to_value()))
            .collect();
        Self {
            classes: classes::ClassTable::native(&registry),
            property_references: Default::default(),
            closure_captures: Vec::new(),
            initializer_scope: None,
            argument_origin: calls::callback_binding::ArgumentOrigin::Php,
            prelinked_classes: std::collections::HashSet::new(),
            function_table: function_table::FunctionTable::new(),
            unit_caches: unit_caches::UnitCaches::new(),
            globals: globals::Globals::new(),
            static_variables: static_variables::StaticVariables::new(),
            diagnostic_reporter: diagnostic_reporter::DiagnosticReporter::new(),
            diagnostic_location: None,
            include_state: include_state::IncludeState::new(),
            call_stack: calls::call_stack::CallStack::new(),
            finally_stack: Vec::new(),
            executed_instructions: 0,
            registry,
            output: vec![],
            output_writer: output::Output::new(),
            registers: calls::registers::Registers::new(),
            constants: constant_table::ConstantTable::new(constants),
            frame_limit: DEFAULT_FRAME_LIMIT,
            status: 0,
            error_handlers: error_handlers::ErrorHandlers::default(),
            failed_native: None,
            state: RequestState::default(),
        }
    }

    pub fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.include_state.set_loader(loader)
    }

    pub fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.output_writer.set_sink(sink)
    }

    /// Defines a global variable before the script starts, such as a
    /// superglobal the runtime fills from the request.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_string(), value);
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

    fn out(&mut self, bytes: &[u8]) {
        self.output_writer.write(&mut self.output, bytes);
    }

    /// Run a compiled top-level function on the request heap stack and report its exit status.
    pub async fn run(&mut self, unit: Arc<CompiledUnit>) -> i32 {
        for id in &unit.hoisted {
            if let Some(frame) = unit.functions.get(id.index()) {
                self.function_table.insert(
                    frame.name.to_ascii_lowercase(),
                    crate::function_table::FunctionRef {
                        unit: unit.clone(),
                        index: id.index(),
                    },
                );
            }
        }
        self.include_state.record(unit.file.clone());
        let cache = self.unit_caches.symbols(&unit);
        let class_unit = unit.clone();
        let mut frame = Frame::new(cache, &mut self.registers, unit, None, None, 0);
        for (name, value) in self.globals.iter() {
            if let Some(i) = frame
                .function()
                .locals
                .iter()
                .position(|candidate| frame.unit.names.get(candidate.index()) == Some(name))
            {
                frame.set(&mut self.registers, Reg(i as u16), value.clone());
            }
        }
        self.call_stack.push(frame);
        self.publish_declaration_warnings(&class_unit);
        if let Err(error) = self.hoist_classes(&class_unit) {
            self.unwind(error);
            return self.status;
        }
        if let Err(error) = self.interpret(0).await {
            let error = self.materialize_error(error);
            if self.invoke_exception_handler(&error).await {
                return self.status;
            }
            self.unwind(error)
        }
        self.status
    }

    #[inline]
    fn current(&self) -> Option<&Frame> {
        self.call_stack.current()
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

    #[inline]
    fn set(&mut self, register: Reg, value: Value) {
        if let Some(frame) = self.call_stack.current_mut() {
            frame.set(&mut self.registers, register, value)
        }
    }
}
