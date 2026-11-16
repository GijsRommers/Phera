//! Request-local async interpreter. PHP calls push detachable heap frames.
mod arrays;
mod call_instructions;
mod call_stack;
mod calls;
mod constant_table;
mod constants;
mod control_flow;
mod diagnostic_reporter;
mod dispatch;
mod dynamic_variables;
mod errors;
mod fast;
mod frame;
mod function_table;
mod global_instructions;
mod globals;
mod include_state;
mod includes;
mod inline_stack;
mod interpreter;
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
mod returns;
use path::AccessPath;
mod references;
mod registers;
mod types;
use frame::Frame;
use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Registry, RequestState,
};
use phera_value::{CollectDiagnostics, Diagnostics, PhpArray, Value, to_php_string};
use std::{cell::RefCell, rc::Rc, sync::Arc};
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
type SymbolCache = Rc<RefCell<Vec<Option<Rc<pending::Callee>>>>>;

/// One request interpreter; PHP calls use heap frames rather than Rust recursion.
pub struct Vm {
    function_table: function_table::FunctionTable,
    globals: globals::Globals,
    diagnostic_reporter: diagnostic_reporter::DiagnosticReporter,
    include_state: include_state::IncludeState,
    call_stack: call_stack::CallStack,
    /// Number of dispatched instructions, for reproducible benchmark reports.
    pub executed_instructions: u64,
    registry: Arc<Registry>,
    /// All PHP output emitted during this request.
    pub output: Vec<u8>,
    sink: Option<Box<dyn OutputSink>>,
    registers: Vec<Option<Value>>,
    constants: constant_table::ConstantTable,
    /// PHP call depth limit, independent of the native Rust stack.
    pub frame_limit: usize,
    status: i32,
    /// What extensions remember during this request (response headers, ...).
    state: RequestState,
}
impl Vm {
    pub fn new(registry: Arc<Registry>) -> Self {
        let constants = registry
            .constants()
            .map(|(n, v)| (n.to_owned(), v.to_value()))
            .collect();
        Self {
            function_table: function_table::FunctionTable::new(),
            globals: globals::Globals::new(),
            diagnostic_reporter: diagnostic_reporter::DiagnosticReporter::new(),
            include_state: include_state::IncludeState::new(),
            call_stack: call_stack::CallStack::new(),
            executed_instructions: 0,
            registry,
            output: vec![],
            sink: None,
            registers: vec![],
            constants: constant_table::ConstantTable::new(constants),
            frame_limit: DEFAULT_FRAME_LIMIT,
            status: 0,
            state: RequestState::default(),
        }
    }
    pub fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.include_state.loader = Some(loader)
    }
    pub fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.sink = Some(sink)
    }
    /// Defines a global variable before the script starts, such as a
    /// superglobal the runtime fills from the request.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals.globals.insert(name.to_string(), value);
    }

    /// The extensions' per-request state, for the runtime to read once the
    /// script has finished.
    pub fn state_mut(&mut self) -> &mut RequestState {
        &mut self.state
    }

    pub fn set_args(&mut self, args: &[String]) {
        let mut a = PhpArray::new();
        for arg in args {
            let _ = a.push(Value::from(arg.clone()));
        }
        self.globals
            .globals
            .insert("argv".into(), Value::Array(Rc::new(a)));
        self.globals
            .globals
            .insert("argc".into(), Value::Int(args.len() as i64));
    }
    fn out(&mut self, b: &[u8]) {
        if let Some(s) = &mut self.sink {
            s.write(b)
        }
        self.output.extend_from_slice(b)
    }
    pub async fn run(&mut self, unit: Arc<CompiledUnit>) -> i32 {
        for id in &unit.hoisted {
            if let Some(f) = unit.functions.get(id.index()) {
                self.function_table.functions.insert(
                    f.name.to_ascii_lowercase(),
                    crate::function_table::FunctionRef {
                        unit: unit.clone(),
                        index: id.index(),
                    },
                );
            }
        }
        self.include_state.included.insert(unit.file.clone());
        let cache = self.code_cache(&unit);
        let mut frame = Frame::new(cache, &mut self.registers, unit, None, None);
        for (name, v) in &self.globals.globals {
            if let Some(i) = frame
                .function()
                .locals
                .iter()
                .position(|n| frame.unit.names.get(n.index()) == Some(name))
            {
                frame.set(&mut self.registers, Reg(i as u16), v.clone());
            }
        }
        self.call_stack.frames.push(frame);
        if let Err(e) = self.interpret(0).await {
            self.unwind(e)
        }
        self.status
    }
    /// Reuse frame-owned call buffers after return. Recursion grows by depth,
    /// rather than allocating fresh preparation buffers for every invocation.
    fn make_frame(
        &mut self,
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Frame {
        let cache = if let Some(frame) = self
            .current()
            .filter(|frame| Arc::ptr_eq(&frame.unit, &unit))
        {
            frame.cache.clone()
        } else {
            self.code_cache(&unit)
        };
        if let Some(mut frame) = self.call_stack.recycled_frames.pop() {
            frame.unit = unit;
            frame.cache = cache;
            frame.function_id = function_id;
            frame.destination = destination;
            frame.ip = 0;
            frame.base = self.registers.len();
            frame.count = frame.function().registers as usize;
            frame.arguments.clear();
            frame.calls.clear();
            frame.dynamic =
                dynamic_variables::DynamicVariables::new(frame.function().dynamic_scope);
            frame.iterators.resize(frame.function().iterators);
            self.registers
                .resize_with(frame.base + frame.count, || None);
            frame
        } else {
            Frame::new(cache, &mut self.registers, unit, function_id, destination)
        }
    }

    fn code_cache(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        self.function_table
            .code_caches
            .entry(Arc::as_ptr(unit) as usize)
            .or_insert_with(|| Rc::new(RefCell::new(vec![None; unit.symbols.len()])))
            .clone()
    }
    #[inline]
    fn current(&self) -> Option<&Frame> {
        self.call_stack.frames.last()
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
    fn get(&self, r: Reg) -> Value {
        self.current()
            .map(|f| f.get(&self.registers, r))
            .unwrap_or_default()
    }
    #[inline]
    fn set(&mut self, r: Reg, v: Value) {
        if let Some(f) = self.call_stack.frames.last_mut() {
            f.set(&mut self.registers, r, v)
        }
    }
    fn location(&self) -> (String, usize) {
        self.current()
            .map(|f| (f.unit.file.clone(), f.line()))
            .unwrap_or(("Unknown".into(), 0))
    }
    /// Publish diagnostics while retaining their allocation for the next operator.
    fn diagnostics_flush(&mut self, mut diagnostics: CollectDiagnostics) {
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.operator_diagnostics = diagnostics;
    }
    /// Drain entries without discarding the reusable allocation.
    fn diagnostics_flush_reuse(&mut self, diagnostics: &mut CollectDiagnostics) {
        for (kind, message) in diagnostics.entries.drain(..) {
            let kind = match kind {
                phera_value::DiagnosticKind::Warning => "Warning",
                phera_value::DiagnosticKind::Notice => "Notice",
                phera_value::DiagnosticKind::Deprecated => "Deprecated",
            };
            self.diagnostic(kind, message);
        }
    }

    fn warn(&mut self, message: String) {
        self.diagnostic("Warning", message)
    }
    fn diagnostic(&mut self, kind: &str, message: String) {
        if self.diagnostic_reporter.suppression == 0 {
            let (file, line) = self.location();
            let bytes = format!("\n{kind}: {message} in {file} on line {line}\n").into_bytes();
            if let Some(capture) = &mut self.diagnostic_reporter.diagnostic_capture {
                capture.push(bytes)
            } else {
                self.out(&bytes)
            }
        }
    }
    fn unwind(&mut self, e: NativeError) {
        self.diagnostic_reporter.suppression = 0;
        if let NativeError::Throw { class, message } = e {
            let (file, line) = self.location();
            let mut text = format!(
                "\nFatal error: Uncaught {class}: {message} in {file}:{line}\nStack trace:\n"
            );
            let mut n = 0;
            for (f, child) in self
                .call_stack
                .frames
                .iter()
                .rev()
                .skip(1)
                .zip(self.call_stack.frames.iter().rev())
            {
                text.push_str(&format!(
                    "#{n} {}({}): {}()\n",
                    f.unit.file,
                    f.line(),
                    child.function().name
                ));
                n += 1;
            }
            text.push_str(&format!(
                "#{n} {{main}}\n  thrown in {file} on line {line}\n"
            ));
            self.out(text.as_bytes());
            self.status = 255;
        }
        self.call_stack.frames.clear();
    }
}
impl Diagnostics for Vm {
    fn warning(&mut self, m: String) {
        self.diagnostic("Warning", m)
    }
    fn notice(&mut self, m: String) {
        self.diagnostic("Notice", m)
    }
    fn deprecated(&mut self, m: String) {
        self.diagnostic("Deprecated", m)
    }
}
impl Context for Vm {
    fn write(&mut self, b: &[u8]) {
        self.out(b)
    }
    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }
    fn state(&mut self) -> &mut RequestState {
        &mut self.state
    }

    fn strict_types(&self) -> bool {
        self.current().is_some_and(|f| f.function().strict_types)
    }
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        args: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            if self.call_stack.callback_depth >= 32 {
                return Err(NativeError::error("Maximum native callback depth exceeded"));
            }
            self.call_stack.callback_depth += 1;
            let result = async {
                let mut d = CollectDiagnostics::default();
                let name = to_php_string(callable, &mut d)?.to_string_lossy();
                let stop = self.call_stack.frames.len();
                if let Some(function) = self
                    .function_table
                    .functions
                    .get(&name.to_ascii_lowercase())
                    .cloned()
                {
                    self.push_call(function.unit, function.index, None, args)?;
                    self.interpret(stop).await
                } else {
                    Box::pin(self.native(&name, args)).await
                }
            }
            .await;
            self.call_stack.callback_depth -= 1;
            result
        })
    }
}
