//! Request-local async interpreter. PHP calls push detachable heap frames.
mod calls;
mod frame;
mod includes;
mod interpreter;
mod ops;
mod places;
mod types;
use frame::Frame;
use phera_bytecode::*;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeFn, NativeResult, Registry, prepare_arguments,
};
use phera_value::{
    ArrayKey, CollectDiagnostics, Diagnostics, PhpArray, Value, to_bool, to_php_string,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
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
pub struct Vm {
    registry: Arc<Registry>,
    loader: Option<Arc<dyn CodeLoader>>,
    included: std::collections::HashSet<String>,
    pub output: Vec<u8>,
    sink: Option<Box<dyn OutputSink>>,
    frames: Vec<Frame>,
    functions: HashMap<String, (Arc<CompiledUnit>, usize)>,
    constants: HashMap<String, Value>,
    globals: HashMap<String, Value>,
    statics: HashMap<(String, String), Value>,
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
            registry,
            loader: None,
            included: std::collections::HashSet::new(),
            output: vec![],
            sink: None,
            frames: vec![],
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
            if let Some(f) = unit.functions.get(*id) {
                self.functions
                    .insert(f.name.to_ascii_lowercase(), (unit.clone(), *id));
            }
        }
        self.included.insert(unit.file.clone());
        let mut frame = Frame::new(unit, None, None);
        for (name, v) in &self.globals {
            if let Some(i) = frame.function().locals.iter().position(|n| n == name) {
                frame.set(Reg(i as u16), v.clone());
            }
        }
        self.frames.push(frame);
        if let Err(e) = self.interpret(0).await {
            self.unwind(e)
        }
        self.status
    }
    fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }
    fn get(&self, r: Reg) -> Value {
        self.current().map(|f| f.get(r)).unwrap_or_default()
    }
    fn set(&mut self, r: Reg, v: Value) {
        if let Some(f) = self.frames.last_mut() {
            f.set(r, v)
        }
    }
    fn location(&self) -> (String, usize) {
        self.current()
            .map(|f| (f.unit.file.clone(), f.line()))
            .unwrap_or(("Unknown".into(), 0))
    }
    fn diagnostics_flush(&mut self, d: CollectDiagnostics) {
        for (k, m) in d.entries {
            use phera_value::DiagnosticKind::*;
            self.diagnostic(
                match k {
                    Warning => "Warning",
                    Notice => "Notice",
                    Deprecated => "Deprecated",
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
