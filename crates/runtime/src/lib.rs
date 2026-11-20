//! Ties the pieces together: loads the extensions, compiles files through a
//! shared cache, and runs each script or web request in a VM of its own.
//!
//! A [`Runtime`] is cheap to clone and safe to share between threads. Every
//! worker thread holds a clone; all clones use the same extension registry
//! and the same compiled code.

mod cache;
mod constant_calls;
mod environment;
mod error;
#[cfg(test)]
mod extension_introspection_tests;
mod http;
#[cfg(test)]
mod ini_tests;
#[cfg(test)]
mod native_parameter_tests;
#[cfg(test)]
mod native_trace_tests;
#[cfg(test)]
mod process_oracle_tests;

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use phera_bytecode::CompiledUnit;
use phera_dotenv::Environment;
use phera_ext_http::{RequestHeaders, ResponseHead, Sapi, UploadedFiles};
use phera_ext_standard::environment::{RequestEnvironment, php_array};
use phera_extension_api::Registry;
use phera_value::{PhpArray, Value};
use phera_vm::{NativeTables, Vm};

use cache::{CodeCache, FileMode};

pub use error::RuntimeError;
pub use http::{HttpRequest, HttpResponse};
pub use phera_vm::OutputSink;

#[derive(Clone)]
pub struct Runtime {
    registry: Arc<Registry>,
    web_registry: Arc<Registry>,
    cache: CodeCache,
    web_cache: CodeCache,
    environment: Arc<Environment>,
    startup_ini: Arc<[(String, String)]>,
    web_development: bool,
    no_process: bool,
}

/// A web worker links the process's native definitions once on its own thread.
pub struct WebWorker {
    runtime: Runtime,
    native: NativeTables,
    environment: Rc<PhpArray>,
}

impl WebWorker {
    pub async fn handle_request(
        &self,
        script: &Path,
        document_root: &Path,
        request: HttpRequest,
    ) -> HttpResponse {
        self.runtime
            .handle_web_request(self, script, document_root, request)
            .await
    }
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
        // mysql_async builds its own ClientConfig and requires a process-default provider.
        static MYSQL_TLS: std::sync::Once = std::sync::Once::new();
        MYSQL_TLS.call_once(|| {
            let _ = phera_io_pool::tls::provider()
                .as_ref()
                .clone()
                .install_default();
        });
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        registry.class(phera_extension_api::ClassEntry {
            parent: Some("LogicException"),
            ..phera_extension_api::ClassEntry::class("Phera\\NotSupportedException")
        });
        registry.load(&phera_ext_standard::Standard);
        registry.load(&phera_ext_tokenizer::Tokenizer);
        registry.load(&phera_ext_streams::Streams);
        registry.load(&phera_ext_process::Process);
        registry.load(&phera_ext_process::Posix);
        registry.load(&phera_ext_zlib::Zlib);
        registry.load(&phera_ext_bcmath::BcMath);
        registry.load(&phera_ext_gmp::Gmp);
        registry.load(&phera_ext_ctype::Ctype);
        registry.load(&phera_ext_date::Date);
        registry.load(&phera_ext_curl::Curl);
        registry.load(&phera_ext_dom::Libxml);
        registry.load(&phera_ext_dom::Dom);
        registry.load(&phera_ext_filter::Filter);
        registry.load(&phera_ext_json::Json);
        registry.load(&phera_ext_mbstring::Mbstring);
        registry.php_module("iconv");
        registry.load(&phera_ext_pcre::Pcre);
        registry.load(&phera_ext_pdo::Pdo);
        registry.php_module("pdo_mysql");
        registry.php_module("pdo_pgsql");
        registry.php_module("pdo_sqlite");
        registry.load(&phera_ext_sqlite3::Sqlite3);
        registry.load(&phera_ext_redis::Redis);
        registry.load(&phera_ext_http::Http);
        http::register_request_parse_body(&mut registry);
        registry.load(&phera_ext_hash::Hash);
        registry.load(&phera_ext_openssl::OpenSsl);
        registry.load(&phera_ext_sodium::Sodium);
        registry.load(&phera_ext_random::Random);
        registry.load(&phera_ext_reflection::Reflection);
        registry.load(&phera_ext_serialize::Serialize);
        Vm::register_fiber_classes(&mut registry);
        registry.load(&phera_ext_spl::Spl);
        registry.load(&phera_ext_session::Session);
        let mut web_registry = registry.clone();
        phera_ext_http::register_web(&mut web_registry);
        phera_ext_process::register_cli_process_control(&mut registry);
        registry.constant("PHP_SAPI", Sapi::Cli.name());
        let registry = Arc::new(registry);
        let web_registry = Arc::new(web_registry);
        Runtime {
            web_cache: CodeCache::new(web_registry.clone(), FileMode::Web),
            web_registry,
            cache: CodeCache::new(registry.clone(), FileMode::Cli),
            registry,
            environment: Arc::new(environment),
            startup_ini: Arc::new([]),
            web_development: false,
            no_process: false,
        }
    }

    /// Settings supplied by the host before each request begins.
    pub fn with_startup_ini(mut self, settings: Vec<(String, String)>) -> Self {
        self.startup_ini = settings.into();
        self
    }

    /// Start web requests with display enabled for local development.
    pub fn with_web_development(mut self, enabled: bool) -> Self {
        self.web_development = enabled;
        self
    }

    pub fn without_web_processes(mut self, disabled: bool) -> Self {
        self.no_process = disabled;
        self
    }

    /// Compiles a file, or returns it from the cache.
    pub async fn compile_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        self.web_cache.compile(path.as_ref().to_path_buf()).await
    }

    /// Runs a script as the command line does: `arguments` become `$argv`,
    /// and output goes to `sink` as it is produced.
    pub async fn run_file(
        &self,
        path: impl AsRef<Path>,
        arguments: &[String],
        sink: Option<Box<dyn OutputSink>>,
    ) -> Result<RunResult, RuntimeError> {
        let compiled = self.cache.compile(path.as_ref().to_path_buf()).await;
        let mut vm = self.new_cli_vm();
        if let Some(sink) = sink {
            vm.set_sink(sink);
        }
        let unit = match compiled {
            Ok(unit) => unit,
            Err(RuntimeError::Compile(error)) => {
                vm.report_entry_compile_failure(error);
                return Ok(RunResult {
                    executed_instructions: 0,
                    output: vm.output,
                    status: 255,
                });
            }
            Err(error) => return Err(error),
        };
        vm.set_args(arguments);
        let server =
            environment::cli_server(php_array(self.environment.iter()), path.as_ref(), arguments);
        vm.set_global("_SERVER", Value::from(server));
        for name in ["_GET", "_POST", "_COOKIE", "_FILES"] {
            vm.set_global(name, Value::from(PhpArray::new()));
        }
        for (name, stream) in phera_ext_streams::standard_constants() {
            phera_ext_streams::track_standard(vm.state_mut(), &stream);
            vm.set_constant(name, stream);
        }
        let status = vm.run(unit).await;
        Ok(RunResult {
            executed_instructions: vm.executed_instructions,
            output: vm.output,
            status,
        })
    }

    /// Create this on the thread that will run the web requests.
    pub fn web_worker(&self) -> WebWorker {
        WebWorker {
            runtime: self.clone(),
            native: NativeTables::new(self.web_registry.clone()),
            environment: self.inherited_environment(),
        }
    }

    /// Run one standalone web request. Use [`Self::web_worker`] to serve successive requests.
    pub async fn handle_request(
        &self,
        script: &Path,
        document_root: &Path,
        request: HttpRequest,
    ) -> HttpResponse {
        self.web_worker()
            .handle_request(script, document_root, request)
            .await
    }

    async fn handle_web_request(
        &self,
        worker: &WebWorker,
        script: &Path,
        document_root: &Path,
        request: HttpRequest,
    ) -> HttpResponse {
        let compiled = match self.web_cache.compile(script.to_path_buf()).await {
            Ok(unit) => Ok(unit),
            Err(RuntimeError::Compile(error)) => Err(error),
            Err(RuntimeError::Read { .. }) => return HttpResponse::plain(404, "File not found.\n"),
            Err(_) => return HttpResponse::plain(500, ""),
        };

        // A form with files is read before the script starts, as in PHP,
        // and its files are written on a thread that may block. PHP gives
        // such a request no raw body to read afterwards.
        let mut request = request;
        let boundary = http::form_boundary(&request);
        let (form, form_warning) = match boundary {
            http::Boundary::Valid(boundary) => {
                let body = std::mem::take(&mut request.body);
                (http::receive_form(body, boundary).await, None)
            }
            http::Boundary::Invalid(warning) => (None, Some(warning)),
            http::Boundary::OtherMediaType => (None, None),
        };

        let mut vm = self.new_vm(&worker.native, Sapi::Web, worker.environment.clone());
        let uploads = http::define_superglobals(
            &mut vm,
            &request,
            script,
            document_root,
            worker.environment.as_ref().clone(),
            form,
        );
        let buffered_post = request.method == "POST"
            && request
                .header("Content-Type")
                .is_some_and(|content_type| !http::is_multipart(content_type));
        *vm.state_mut().get_or_default::<RequestHeaders>() = RequestHeaders {
            fields: request.headers,
            method: request.method,
            target: request.target,
            protocol: request.protocol,
        };
        vm.prepare_web_headers_with(ResponseHead::prepare, ResponseHead::set_error_status);
        vm.configure_web_diagnostics(self.web_development, RequestHeaders::request_line);
        *vm.state_mut().get_or_default::<UploadedFiles>() = UploadedFiles::new(uploads);
        phera_ext_streams::set_request(
            vm.state_mut(),
            script.parent().map(Path::to_path_buf),
            request.body.into(),
        );
        if buffered_post && let Some(input) = phera_ext_streams::request_input(vm.state_mut()) {
            input.mark_post_read();
        }
        if let Some(warning) = form_warning {
            vm.request_startup_warning(warning);
        }
        match compiled {
            Ok(unit) => {
                vm.run(unit).await;
            }
            Err(error) => {
                vm.report_entry_compile_failure(error);
                ResponseHead::prepare(vm.state_mut());
            }
        }
        let head = vm.state_mut().take::<ResponseHead>().unwrap_or_default();
        HttpResponse {
            status: head.status,
            headers: head.headers,
            body: std::mem::take(&mut vm.output),
        }
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
        let unit = single_threaded_executor()?
            .block_on(self.cache.compile(path.as_ref().to_path_buf()))?;
        Ok(unit.to_string())
    }

    fn new_cli_vm(&self) -> Vm {
        self.new_vm(
            &NativeTables::new(self.registry.clone()),
            Sapi::Cli,
            self.inherited_environment(),
        )
    }

    /// Render compilation failure with the CLI request's startup settings.
    pub fn cli_compile_output(&self, error: phera_compiler::CompileError) -> Vec<u8> {
        let mut vm = self.new_cli_vm();
        vm.report_entry_compile_failure(error);
        vm.output
    }

    fn inherited_environment(&self) -> Rc<PhpArray> {
        Rc::new(php_array(self.environment.iter()))
    }

    fn new_vm(&self, native: &NativeTables, sapi: Sapi, environment: Rc<PhpArray>) -> Vm {
        let mut vm = Vm::with_native_tables(native);
        *vm.state_mut().get_or_default::<Sapi>() = sapi;
        if matches!(sapi, Sapi::Web) {
            vm.use_web_execution_time();
            phera_ext_process::configure_web(vm.state_mut(), self.no_process);
        }
        for (name, value) in self.startup_ini.iter() {
            vm.set_startup_ini(name, value);
        }
        // A script's writes copy the inherited array before changing it.
        vm.set_global("_ENV", Value::Array(environment));
        *vm.state_mut().get_or_default::<RequestEnvironment>() =
            RequestEnvironment::new(self.environment.clone());
        let cache = match sapi {
            Sapi::Cli => &self.cache,
            Sapi::Web => &self.web_cache,
        };
        vm.set_loader(Arc::new(cache.clone()));
        vm
    }
}

fn single_threaded_executor() -> Result<tokio::runtime::Runtime, RuntimeError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeError::Internal(error.to_string()))
}
