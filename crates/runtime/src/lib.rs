//! Ties the pieces together: loads the extensions, compiles files through a
//! shared cache, and runs each script or web request in a VM of its own.
//!
//! A [`Runtime`] is cheap to clone and safe to share between threads. Every
//! worker thread holds a clone; all clones use the same extension registry
//! and the same compiled code.

pub mod adapters;
mod boot;
mod boot_changes;
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
mod platform;
mod preload;
#[cfg(test)]
mod process_oracle_tests;
mod snapshots;
mod worker_code_cache;

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
use worker_code_cache::{RequestCodeCache, WorkerCodeCache};

pub use boot::BootReport;
pub use error::RuntimeError;
pub use http::{HttpRequest, HttpResponse};
pub use phera_extension_api::web_log::flush as flush_web_log;
pub use phera_vm::OutputSink;
pub use preload::PreloadSummary;
pub use snapshots::{remembered as snapshot_remembered, requests_from_the_top};

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
    /// Whether a worker declares in its snapshot the classes its requests
    /// autoloaded (`--no-class-warmup` turns this off).
    class_warmup: bool,
    /// When included files, and the files of a snapshot, are checked for
    /// changes: opcache's settings from the start-up INI.
    include_policy: phera_vm::IncludeCachePolicy,
    /// Scripts that run PHP source Phera ships in place of the file (an
    /// adapter's entry script), by the script's path.
    entry_scripts: Arc<std::collections::HashMap<std::path::PathBuf, &'static str>>,
}

/// A web worker links the process's native definitions once on its own
/// thread, and keeps the snapshots of the scripts it booted. Cheap to clone:
/// the clones share everything, on the same thread.
#[derive(Clone)]
pub struct WebWorker {
    runtime: Runtime,
    /// The worker's number, for its log lines.
    index: usize,
    native: Rc<NativeTables>,
    environment: Rc<PhpArray>,
    snapshots: Rc<snapshots::WorkerSnapshots>,
    /// Whether this worker runs on a `LocalSet`, where a boot can run in
    /// the background (`phera serve`); see [`WebWorker::with_background_boots`].
    background_boots: bool,
    code: Rc<WorkerCodeCache>,
}

impl WebWorker {
    /// Lets this worker boot scripts in the background (a first request
    /// that reached `Phera\snapshot()`, a changed file). Only for a worker
    /// whose requests run on a Tokio `LocalSet`, as `phera serve`'s do.
    pub fn with_background_boots(mut self) -> Self {
        self.background_boots = true;
        self
    }

    /// The worker's number.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The classes this worker's requests of `script` autoloaded so far.
    pub fn warmup_classes(&self, script: &Path) -> Vec<String> {
        self.snapshots.warmup_classes(script)
    }

    /// How many warm-up boots this worker ran for `script` (a boot a
    /// changed file started does not count).
    pub fn warmup_boots(&self, script: &Path) -> u32 {
        self.snapshots.warmup_boots(script)
    }

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
        // Thrown by `Phera\snapshot()` when the state cannot be kept.
        registry.class(phera_extension_api::ClassEntry {
            parent: Some("RuntimeException"),
            ..phera_extension_api::ClassEntry::class("Phera\\SnapshotException")
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
        registry.load(&phera_ext_xml::Xml);
        registry.load(&phera_ext_xmlreader::XmlReader);
        registry.load(&phera_ext_xmlwriter::XmlWriter);
        registry.load(&phera_ext_fileinfo::FileInfo);
        registry.load(&phera_ext_gd::Gd);
        registry.load(&phera_ext_gd::ImageMetadata);
        registry.load(&phera_ext_gd::ExifImageType);
        registry.load(&phera_ext_filter::Filter);
        registry.load(&phera_ext_intl::Intl);
        registry.load(&phera_ext_json::Json);
        registry.load(&phera_ext_mbstring::Mbstring);
        registry.php_module("iconv");
        registry.load(&phera_ext_pcre::Pcre);
        registry.load(&phera_ext_pdo::Pdo);
        registry.load(&phera_ext_mysqli::Mysqli);
        registry.load(&phera_ext_pgsql::Pgsql);
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
        Vm::register_state_forks(&mut registry);
        registry.load(&phera_ext_spl::Spl);
        registry.load(&phera_ext_simplexml::SimpleXml);
        registry.load(&phera_ext_session::Session);
        adapters::register(&mut registry);
        let mut web_registry = registry.clone();
        phera_ext_http::register_web(&mut web_registry);
        web_registry.sapi_ini("phera_server");
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
            class_warmup: true,
            include_policy: phera_vm::IncludeCachePolicy::default(),
            entry_scripts: Arc::default(),
        }
    }

    /// Runs `source` whenever a request goes to `script`, compiled under
    /// that file's name (see [`adapters`]).
    pub fn with_entry_script(mut self, script: std::path::PathBuf, source: &'static str) -> Self {
        Arc::make_mut(&mut self.entry_scripts).insert(script, source);
        self
    }

    /// The compiled code a request to `script` runs.
    async fn entry_unit(
        &self,
        code: &RequestCodeCache,
        script: &Path,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        match self.entry_scripts.get(script) {
            Some(source) => self
                .web_cache
                .compile_shipped(source, &script.to_string_lossy()),
            None => code.entry(script, self.include_policy).await,
        }
    }

    /// Settings supplied by the host before each request begins.
    pub fn with_startup_ini(mut self, settings: Vec<(String, String)>) -> Self {
        for (name, value) in &settings {
            self.include_policy.set_ini(name, value.as_bytes());
        }
        self.startup_ini = settings.into();
        self
    }

    /// Display diagnostics and validate source files on each web request.
    pub fn with_web_development(mut self, enabled: bool) -> Self {
        self.web_development = enabled;
        if enabled {
            // As for includes: every request looks for changed files.
            self.include_policy.validate_timestamps = true;
            self.include_policy.revalidate_freq = std::time::Duration::ZERO;
        }
        self
    }

    /// What a request does while a changed snapshot boots again.
    fn stale_snapshots(&self) -> snapshots::Stale {
        if self.web_development {
            snapshots::Stale::RunFromTheTop
        } else {
            snapshots::Stale::ForkOld
        }
    }

    pub fn without_web_processes(mut self, disabled: bool) -> Self {
        self.no_process = disabled;
        self
    }

    /// Turn off declaring in the snapshot the classes requests autoload.
    pub fn without_class_warmup(mut self, disabled: bool) -> Self {
        self.class_warmup = !disabled;
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
    /// Create this on the thread that will run the web requests; `index`
    /// is the worker's number.
    pub fn web_worker(&self, index: usize) -> WebWorker {
        WebWorker {
            runtime: self.clone(),
            index,
            native: Rc::new(NativeTables::new(self.web_registry.clone())),
            environment: self.inherited_environment(),
            snapshots: Default::default(),
            background_boots: false,
            code: Rc::new(WorkerCodeCache::new(self.web_cache.clone())),
        }
    }

    /// Run one standalone web request. Use [`Self::web_worker`] to serve successive requests.
    pub async fn handle_request(
        &self,
        script: &Path,
        document_root: &Path,
        request: HttpRequest,
    ) -> HttpResponse {
        self.web_worker(0)
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
        let code = Rc::new(RequestCodeCache::new(worker.code.clone()));
        let compiled = match self.entry_unit(&code, script).await {
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

        let (plan, reboot) =
            worker
                .snapshots
                .plan(script, self.include_policy, self.stale_snapshots());
        if reboot {
            worker.boot_in_background(script, document_root);
        }
        let mut carried = None;
        let mut warmup_before = None;
        let mut vm = match (plan, &compiled) {
            (snapshots::Plan::Fork(snapshot, changes), Ok(_)) => match snapshot.fork() {
                Ok(mut vm) => {
                    carried = Some(changes.carry(&vm));
                    if self.class_warmup {
                        let sink = worker.snapshots.warmup_sink(script);
                        warmup_before = Some(sink.borrow().len());
                        vm.set_warmup_sink(sink);
                    }
                    vm
                }
                Err(error) => {
                    eprintln!(
                        "phera: worker {}: DEFECT: this request to {} runs from the top: {error}",
                        worker.index,
                        script.display()
                    );
                    snapshots::count_from_the_top();
                    self.new_classic_vm(worker)
                }
            },
            _ => self.new_classic_vm(worker),
        };
        let forked = vm.is_fork();
        vm.set_local_loader(code);
        let uploads = http::define_superglobals(
            &mut vm,
            &request,
            script,
            document_root,
            worker.environment.as_ref().clone(),
            form,
        );
        if let Some(carried) = carried {
            carried.apply(&mut vm);
        }
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
            Ok(_) if forked => {
                vm.resume().await;
            }
            Ok(unit) => {
                vm.run(unit).await;
                // The first start of a script that snapshots cannot know it
                // does: the request that first reaches the call has the
                // worker boot it.
                if vm.snapshot_called() && !worker.snapshots.knows(script) {
                    snapshots::remember(script);
                    worker.boot_in_background(script, document_root);
                }
            }
            Err(error) => {
                vm.report_entry_compile_failure(error);
                ResponseHead::prepare(vm.state_mut());
            }
        }
        // A forked request that autoloaded new classes grows the warm-up
        // list; once the list settles, the worker boots again in the
        // background and the new snapshot declares the classes.
        if let Some(before) = warmup_before {
            let grew = worker.snapshots.warmup_sink(script).borrow().len() > before;
            if worker.snapshots.note_forked_request(script, grew) {
                worker.boot_in_background(script, document_root);
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

    /// A request VM that runs its script from the top.
    fn new_classic_vm(&self, worker: &WebWorker) -> Vm {
        let mut vm = self.new_vm(&worker.native, Sapi::Web, worker.environment.clone());
        vm.set_snapshot_mode(phera_vm::SnapshotMode::Classic);
        vm
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
        if matches!(sapi, Sapi::Web) && self.web_development {
            vm.set_startup_ini("opcache.validate_timestamps", "1");
            vm.set_startup_ini("opcache.revalidate_freq", "0");
        }
        // A script's writes copy the inherited array before changing it.
        vm.set_global("_ENV", Value::Array(environment));
        *vm.state_mut().get_or_default::<RequestEnvironment>() =
            RequestEnvironment::new(self.environment.clone());
        if matches!(sapi, Sapi::Cli) {
            vm.set_local_loader(Rc::new(self.cache.clone()));
        }
        vm
    }
}

fn single_threaded_executor() -> Result<tokio::runtime::Runtime, RuntimeError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeError::Internal(error.to_string()))
}
