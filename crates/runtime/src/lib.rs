//! Ties the pieces together: loads the extensions, compiles files through a
//! shared cache, and runs each script or web request in a VM of its own.
//!
//! A [`Runtime`] is cheap to clone and safe to share between threads. Every
//! worker thread holds a clone; all clones use the same extension registry
//! and the same compiled code.

mod cache;
mod environment;
mod error;
mod http;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use phera_bytecode::CompiledUnit;
use phera_dotenv::Environment;
use phera_ext_http::ResponseHead;
use phera_ext_standard::environment::{RequestEnvironment, php_array};
use phera_extension_api::{LocalFuture, Registry};
use phera_value::Value;
use phera_vm::{CodeLoader, Vm};

use cache::CodeCache;

pub use error::RuntimeError;
pub use http::{HttpRequest, HttpResponse};
pub use phera_vm::OutputSink;

#[derive(Clone)]
pub struct Runtime {
    registry: Arc<Registry>,
    cache: CodeCache,
    environment: Arc<Environment>,
}

/// What a command-line script did.
pub struct RunResult {
    /// Instructions executed, including those of included files.
    pub executed_instructions: u64,
    pub output: Vec<u8>,
    /// The exit status: 0, the argument of `exit()`, or 255 after an
    /// uncaught error.
    pub status: i32,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::with_environment(Environment::process())
    }

    pub fn with_environment(environment: Environment) -> Self {
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        registry.load(&phera_ext_standard::Standard);
        registry.load(&phera_ext_ctype::Ctype);
        registry.load(&phera_ext_date::Date);
        registry.load(&phera_ext_filter::Filter);
        registry.load(&phera_ext_json::Json);
        registry.load(&phera_ext_pcre::Pcre);
        registry.load(&phera_ext_pdo::Pdo);
        registry.load(&phera_ext_http::Http);
        registry.load(&phera_ext_hash::Hash);
        registry.load(&phera_ext_random::Random);
        Runtime {
            registry: Arc::new(registry),
            cache: CodeCache::default(),
            environment: Arc::new(environment),
        }
    }

    /// Compiles a file, or returns it from the cache.
    pub async fn compile_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let path = path.as_ref().to_path_buf();
        let cache = self.cache.clone();
        // Reading and compiling block, and a worker thread must never block:
        // every request on it would wait.
        tokio::task::spawn_blocking(move || cache.load(&path))
            .await
            .map_err(|error| RuntimeError::Internal(error.to_string()))?
    }

    /// Runs a script as the command line does: `arguments` become `$argv`,
    /// and output goes to `sink` as it is produced.
    pub async fn run_file(
        &self,
        path: impl AsRef<Path>,
        arguments: &[String],
        sink: Option<Box<dyn OutputSink>>,
    ) -> Result<RunResult, RuntimeError> {
        let unit = self.compile_file(path.as_ref()).await?;
        let mut vm = self.new_vm();
        vm.set_args(arguments);
        let server =
            environment::cli_server(php_array(self.environment.iter()), path.as_ref(), arguments);
        vm.set_global("_SERVER", Value::from(server));
        if let Some(sink) = sink {
            vm.set_sink(sink);
        }
        let status = vm.run(unit).await;
        Ok(RunResult {
            executed_instructions: vm.executed_instructions,
            output: vm.output,
            status,
        })
    }

    /// Runs `script` for one web request and builds the response from what
    /// it printed and from its `header()` calls.
    ///
    /// Every request gets a new VM, so nothing a request does to globals,
    /// statics or constants is visible to the next one.
    pub async fn handle_request(
        &self,
        script: &Path,
        document_root: &Path,
        request: HttpRequest,
    ) -> HttpResponse {
        let unit = match self.compile_file(script).await {
            Ok(unit) => unit,
            Err(RuntimeError::Read { .. }) => {
                return HttpResponse::plain(404, "File not found.\n");
            }
            Err(error) => return HttpResponse::plain(500, error.to_string()),
        };

        let mut vm = self.new_vm();
        http::define_superglobals(
            &mut vm,
            &request,
            script,
            document_root,
            php_array(self.environment.iter()),
        );
        let exit_status = vm.run(unit).await;

        let head = vm.state_mut().take::<ResponseHead>().unwrap_or_default();
        let mut response = HttpResponse {
            status: head.status,
            headers: head.headers,
            body: std::mem::take(&mut vm.output),
        };
        // An uncaught error is a server error, unless the script had already
        // chosen a status of its own.
        if exit_status == 255 && response.status == 200 {
            response.status = 500;
        }
        if response.header("Content-Type").is_none() {
            response.headers.push((
                "Content-Type".to_string(),
                b"text/html; charset=UTF-8".to_vec(),
            ));
        }
        response
    }

    /// [`Runtime::run_file`] for callers that are not async: starts a
    /// single-threaded Tokio runtime for the duration of the script.
    pub fn run_file_blocking(
        &self,
        path: impl AsRef<Path>,
        arguments: &[String],
        sink: Option<Box<dyn OutputSink>>,
    ) -> Result<RunResult, RuntimeError> {
        let executor = single_threaded_executor()?;
        // PHP values are not `Send`, so a script's future can only run on a
        // `LocalSet`, which keeps its tasks on this thread.
        tokio::task::LocalSet::new().block_on(&executor, self.run_file(path, arguments, sink))
    }

    /// The bytecode listing of a file, for `phera --dump`.
    pub fn dump_file_blocking(&self, path: impl AsRef<Path>) -> Result<String, RuntimeError> {
        let unit = single_threaded_executor()?.block_on(self.compile_file(path))?;
        Ok(unit.to_string())
    }

    fn new_vm(&self) -> Vm {
        let mut vm = Vm::new(self.registry.clone());
        vm.set_global("_ENV", Value::from(php_array(self.environment.iter())));
        *vm.state_mut().get_or_default::<RequestEnvironment>() =
            RequestEnvironment::new(self.environment.clone());
        vm.set_loader(Arc::new(self.clone()));
        vm
    }
}

fn single_threaded_executor() -> Result<tokio::runtime::Runtime, RuntimeError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeError::Internal(error.to_string()))
}

/// `include` and `require` load code through the same cache.
impl CodeLoader for Runtime {
    fn load(&self, path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>> {
        Box::pin(async move {
            self.compile_file(path)
                .await
                .map_err(|error| error.to_string())
        })
    }
}
