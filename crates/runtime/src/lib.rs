//! Shared compilation cache and request-local execution.
use phera_bytecode::CompiledUnit;
use phera_extension_api::Registry;
pub use phera_vm::OutputSink;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Clone)]
pub struct Runtime {
    registry: Arc<Registry>,
    cache: Arc<Mutex<HashMap<PathBuf, Arc<CompiledUnit>>>>,
}
pub struct RunResult {
    /// Instructions executed by this request, including included files.
    pub executed_instructions: u64,
    pub output: Vec<u8>,
    pub status: i32,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
impl Runtime {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        registry.load(&phera_ext_standard::Standard);
        registry.load(&phera_ext_ctype::Ctype);
        registry.load(&phera_ext_json::Json);
        Self {
            registry: Arc::new(registry),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub async fn compile_file(&self, path: impl AsRef<Path>) -> Result<Arc<CompiledUnit>, String> {
        let path = path.as_ref().to_path_buf();
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if let Some(c) = cache.lock().map_err(|e| e.to_string())?.get(&path).cloned() {
                return Ok(c);
            }
            let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let unit = Arc::new(
                phera_compiler::compile(&source, &path.to_string_lossy())
                    .map_err(|e| e.to_string())?,
            );
            cache
                .lock()
                .map_err(|e| e.to_string())?
                .insert(path, unit.clone());
            Ok(unit)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn run_file(
        &self,
        path: impl AsRef<Path>,
        args: &[String],
        sink: Option<Box<dyn OutputSink>>,
    ) -> Result<RunResult, String> {
        let code = self.compile_file(path).await?;
        let mut vm = phera_vm::Vm::new(self.registry.clone());
        vm.set_args(args);
        vm.set_loader(Arc::new(self.clone()));
        if let Some(sink) = sink {
            vm.set_sink(sink)
        }
        let status = vm.run(code).await;
        Ok(RunResult {
            executed_instructions: vm.executed_instructions,
            output: vm.output,
            status,
        })
    }
    pub fn run_file_blocking(
        &self,
        path: impl AsRef<Path>,
        args: &[String],
        sink: Option<Box<dyn OutputSink>>,
    ) -> Result<RunResult, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        tokio::task::LocalSet::new().block_on(&rt, self.run_file(path, args, sink))
    }
    pub fn dump_file_blocking(&self, path: impl AsRef<Path>) -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(self.compile_file(path)).map(|c| c.to_string())
    }
}

impl phera_vm::CodeLoader for Runtime {
    fn load(
        &self,
        path: PathBuf,
    ) -> phera_extension_api::LocalFuture<'_, Result<Arc<CompiledUnit>, String>> {
        Box::pin(self.compile_file(path))
    }
}
#[cfg(test)]
mod tests {
    use super::{OutputSink, Runtime};
    use std::{path::Path, sync::Arc};
    #[tokio::test(flavor = "current_thread")]
    async fn cache_shares_code_and_sink_receives_output() {
        use std::{cell::RefCell, rc::Rc};
        struct Sink(Rc<RefCell<Vec<u8>>>);
        impl OutputSink for Sink {
            fn write(&mut self, b: &[u8]) {
                self.0.borrow_mut().extend_from_slice(b)
            }
        }
        let runtime = Runtime::new();
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/includes/shared.inc");
        let a = runtime.compile_file(&path).await.expect("fixture compiles");
        let b = runtime.compile_file(&path).await.expect("cached fixture");
        assert!(Arc::ptr_eq(&a, &b));
        let output = Rc::new(RefCell::new(vec![]));
        let result = runtime
            .run_file(&path, &[], Some(Box::new(Sink(output.clone()))))
            .await
            .expect("fixture runs");
        assert_eq!(result.status, 0);
        assert_eq!(output.borrow().as_slice(), result.output);
        assert!(result.output.ends_with(b"inc"));
    }
}
