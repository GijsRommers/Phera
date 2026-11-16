//! Request-local async interpreter. PHP calls push detachable heap frames.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{LocalFuture, Registry, RequestState};
use phera_value::{PhpArray, Value};

use crate::frame::Frame;
use crate::path::AccessPath;

/// Compiler invariant failures are PHP errors. Keep literal construction here:
/// formatting a dynamic detail changed code generation enough to miss the speed gate.
macro_rules! internal_error {
    ($detail:literal) => {
        phera_extension_api::NativeError::error(concat!("Phera internal error: ", $detail))
    };
}

mod access_mode;
mod arrays;
mod call_instructions;
mod call_stack;
mod call_state;
mod calls;
mod constant_table;
mod constants;
mod context;
mod control_flow;
mod diagnostic_reporter;
mod diagnostics;
mod dispatch;
mod dynamic_variables;
mod error_traces;
mod error_values;
mod errors;
mod extract;
mod fast;
mod frame;
mod frame_lifecycle;
mod function_table;
mod global_instructions;
mod global_paths;
mod globals;
mod include_state;
mod includes;
mod inline_stack;
mod interpreter;
mod intrinsic_calls;
mod intrinsics;
mod iteration;
mod iterator_instructions;
mod iterators;
mod locals;
mod operators;
mod ops;
mod output;
mod path;
mod path_instructions;
mod path_update;
mod pending;
mod places;
mod references;
mod registers;
mod returns;
mod scope_intrinsics;
mod static_variables;
mod string_offsets;
mod types;
mod unit_caches;
mod unpacking;
mod variadic_names;

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
type SymbolCache = Rc<RefCell<Vec<Option<Rc<call_state::Callee>>>>>;

/// One request interpreter; PHP calls use heap frames rather than Rust recursion.
pub struct Vm {
    function_table: function_table::FunctionTable,
    unit_caches: unit_caches::UnitCaches,
    globals: globals::Globals,
    static_variables: static_variables::StaticVariables,
    diagnostic_reporter: diagnostic_reporter::DiagnosticReporter,
    include_state: include_state::IncludeState,
    call_stack: call_stack::CallStack,
    /// Number of dispatched instructions, for reproducible benchmark reports.
    pub executed_instructions: u64,
    registry: Arc<Registry>,
    /// All PHP output emitted during this request.
    pub output: Vec<u8>,
    output_writer: output::Output,
    registers: registers::Registers,
    constants: constant_table::ConstantTable,
    /// PHP call depth limit, independent of the native Rust stack.
    pub frame_limit: usize,
    status: i32,
    failed_native: Option<error_traces::NativeTrace>,
    /// What extensions remember during this request (response headers, ...).
    state: RequestState,
}

impl Vm {
    /// Create isolated request state from an immutable native registry.
    pub fn new(registry: Arc<Registry>) -> Self {
        let constants = registry
            .constants()
            .map(|(name, value)| (name.to_owned(), value.to_value()))
            .collect();
        Self {
            function_table: function_table::FunctionTable::new(),
            unit_caches: unit_caches::UnitCaches::new(),
            globals: globals::Globals::new(),
            static_variables: static_variables::StaticVariables::new(),
            diagnostic_reporter: diagnostic_reporter::DiagnosticReporter::new(),
            include_state: include_state::IncludeState::new(),
            call_stack: call_stack::CallStack::new(),
            executed_instructions: 0,
            registry,
            output: vec![],
            output_writer: output::Output::new(),
            registers: registers::Registers::new(),
            constants: constant_table::ConstantTable::new(constants),
            frame_limit: DEFAULT_FRAME_LIMIT,
            status: 0,
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
        let cache = self.code_cache(&unit);
        let mut frame = Frame::new(cache, &mut self.registers, unit, None, None);
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
        if let Err(error) = self.interpret(0).await {
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
