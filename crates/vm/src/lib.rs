//! Request-local async interpreter. PHP calls push detachable heap frames.
mod arrays;
mod calls;
mod frame;
mod includes;
mod interpreter;
mod iteration;
mod operators;
mod ops;
mod pending;
mod places;
mod references;
mod registers;
mod types;
use frame::Frame;
use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Registry};
use phera_value::{CollectDiagnostics, Diagnostics, PhpArray, Value, to_php_string};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
#[derive(Clone)]
struct Place {
    base: Reg,
    dynamic: bool,
    path: Vec<Option<Reg>>,
}
impl Place {
    fn from_path(path: &phera_bytecode::Path) -> Self {
        Self {
            base: match path.base {
                phera_bytecode::PathBase::Register(register)
                | phera_bytecode::PathBase::Named(register) => register,
            },
            dynamic: matches!(path.base, phera_bytecode::PathBase::Named(_)),
            path: path
                .steps
                .iter()
                .map(|step| match step {
                    phera_bytecode::Step::Index(register) => Some(*register),
                    phera_bytecode::Step::Append => None,
                })
                .collect(),
        }
    }
}
pub trait CodeLoader: Send + Sync {
    fn load(&self, path: std::path::PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>>;
}
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

pub struct Vm {
    /// Number of dispatched instructions, for reproducible benchmark reports.
    pub executed_instructions: u64,
    code_caches: HashMap<usize, SymbolCache>,
    operator_diagnostics: CollectDiagnostics,
    registry: Arc<Registry>,
    loader: Option<Arc<dyn CodeLoader>>,
    included: std::collections::HashSet<String>,
    pub output: Vec<u8>,
    sink: Option<Box<dyn OutputSink>>,
    registers: Vec<Option<Value>>,
    frames: Vec<Frame>,
    recycled_frames: Vec<Frame>,
    argument_buffers: Vec<Vec<Option<Value>>>,
    functions: HashMap<String, (Arc<CompiledUnit>, usize)>,
    constants: HashMap<String, Value>,
    globals: HashMap<String, Value>,
    statics: HashMap<(usize, Option<usize>, u16), Value>,
    pub frame_limit: usize,
    suppression: usize,
    callback_depth: usize,
    status: i32,
    diagnostic_capture: Option<Vec<Vec<u8>>>,
}
impl Vm {
    pub fn new(registry: Arc<Registry>) -> Self {
        let constants = registry
            .constants()
            .map(|(n, v)| (n.to_owned(), v.to_value()))
            .collect();
        Self {
            executed_instructions: 0,
            code_caches: HashMap::new(),
            operator_diagnostics: CollectDiagnostics::default(),
            registry,
            loader: None,
            included: std::collections::HashSet::new(),
            output: vec![],
            sink: None,
            registers: vec![],
            frames: vec![],
            recycled_frames: vec![],
            argument_buffers: vec![],
            functions: HashMap::new(),
            constants,
            globals: HashMap::new(),
            statics: HashMap::new(),
            frame_limit: DEFAULT_FRAME_LIMIT,
            suppression: 0,
            callback_depth: 0,
            status: 0,
            diagnostic_capture: None,
        }
    }
    pub fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.loader = Some(loader)
    }
    pub fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.sink = Some(sink)
    }
    pub fn set_args(&mut self, args: &[String]) {
        let mut a = PhpArray::new();
        for arg in args {
            let _ = a.push(Value::from(arg.clone()));
        }
        self.globals.insert("argv".into(), Value::Array(Rc::new(a)));
        self.globals
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
                self.functions
                    .insert(f.name.to_ascii_lowercase(), (unit.clone(), id.index()));
            }
        }
        self.included.insert(unit.file.clone());
        let cache = self.code_cache(&unit);
        let mut frame = Frame::new(cache, &mut self.registers, unit, None, None);
        for (name, v) in &self.globals {
            if let Some(i) = frame
                .function()
                .locals
                .iter()
                .position(|n| frame.unit.names.get(n.index()) == Some(name))
            {
                frame.set(&mut self.registers, Reg(i as u16), v.clone());
            }
        }
        self.frames.push(frame);
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
        let cache = self.code_cache(&unit);
        if let Some(mut frame) = self.recycled_frames.pop() {
            frame.unit = unit;
            frame.cache = cache;
            frame.function_id = function_id;
            frame.destination = destination;
            frame.ip = 0;
            frame.base = self.registers.len();
            frame.count = frame.function().registers as usize;
            frame.arguments.clear();
            frame.calls.clear();
            frame.dynamic.clear();
            frame.iterators.clear();
            self.registers
                .resize_with(frame.base + frame.count, || None);
            frame
        } else {
            Frame::new(cache, &mut self.registers, unit, function_id, destination)
        }
    }

    fn code_cache(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        self.code_caches
            .entry(Arc::as_ptr(unit) as usize)
            .or_insert_with(|| Rc::new(RefCell::new(vec![None; unit.symbols.len()])))
            .clone()
    }
    fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }
    fn get(&self, r: Reg) -> Value {
        self.current()
            .map(|f| f.get(&self.registers, r))
            .unwrap_or_default()
    }
    fn set(&mut self, r: Reg, v: Value) {
        if let Some(f) = self.frames.last_mut() {
            f.set(&mut self.registers, r, v)
        }
    }
    fn location(&self) -> (String, usize) {
        self.current()
            .map(|f| (f.unit.file.clone(), f.line()))
            .unwrap_or(("Unknown".into(), 0))
    }
    fn diagnostics_flush(&mut self, d: CollectDiagnostics) {
        for (k, m) in d.entries {
            use phera_value::DiagnosticKind;
            self.diagnostic(
                match k {
                    DiagnosticKind::Warning => "Warning",
                    DiagnosticKind::Notice => "Notice",
                    DiagnosticKind::Deprecated => "Deprecated",
                },
                m,
            )
        }
    }
    fn warn(&mut self, message: String) {
        self.diagnostic("Warning", message)
    }
    fn diagnostic(&mut self, kind: &str, message: String) {
        if self.suppression == 0 {
            let (file, line) = self.location();
            let bytes = format!("\n{kind}: {message} in {file} on line {line}\n").into_bytes();
            if let Some(capture) = &mut self.diagnostic_capture {
                capture.push(bytes)
            } else {
                self.out(&bytes)
            }
        }
    }
    fn unwind(&mut self, e: NativeError) {
        self.suppression = 0;
        if let NativeError::Throw { class, message } = e {
            let (file, line) = self.location();
            let mut text = format!(
                "\nFatal error: Uncaught {class}: {message} in {file}:{line}\nStack trace:\n"
            );
            let mut n = 0;
            for (f, child) in self
                .frames
                .iter()
                .rev()
                .skip(1)
                .zip(self.frames.iter().rev())
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
        self.frames.clear();
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
    fn strict_types(&self) -> bool {
        self.current().is_some_and(|f| f.function().strict_types)
    }
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        args: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            if self.callback_depth >= 32 {
                return Err(NativeError::error("Maximum native callback depth exceeded"));
            }
            self.callback_depth += 1;
            let result = async {
                let mut d = CollectDiagnostics::default();
                let name = to_php_string(callable, &mut d)?.to_string_lossy();
                let stop = self.frames.len();
                if let Some((u, id)) = self.functions.get(&name.to_ascii_lowercase()).cloned() {
                    self.push_call(u, id, None, args)?;
                    self.interpret(stop).await
                } else {
                    Box::pin(self.native(&name, args)).await
                }
            }
            .await;
            self.callback_depth -= 1;
            result
        })
    }
}
