//! `phera serve`: answers HTTP requests with PHP scripts and static files.
//!
//! This is the glue between two crates that do not know each other: the
//! HTTP server, which only knows a `Handler`, and the runtime, which only
//! knows how to run a script for a request.

mod byte_range;
mod compress;
mod conditional;
mod config;
mod content_type;
mod forwarded;
mod precompressed;
mod site;
mod static_file;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use phera_runtime::{DebugTimeLimit, HttpRequest, HttpResponse, Runtime, WebWorker};
use phera_server::{Handler, HandlerFactory, HttpVersion, Request, Response, Server, ServerConfig};

use crate::arguments::ServeOptions;
use forwarded::TrustedProxies;
use site::{Resolved, Site};
use static_file::Conditions;

pub(crate) fn serve(options: ServeOptions) -> ExitCode {
    let mut site = match Site::open(&options.path) {
        Ok(site) => site,
        Err(error) => {
            eprintln!("Cannot serve {}: {error}", options.path.display());
            return ExitCode::FAILURE;
        }
    };
    let project = config::project_root(site.document_root()).to_path_buf();
    let settings = match config::read(&project) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut loaded = match options
        .files
        .load(&project, phera_dotenv::Environment::process())
    {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    loaded.paths.extend(settings.path);
    let adapter = Adapter::detect(&site, &mut loaded.environment);
    if !options.service_warmup {
        loaded.environment.insert(
            phera_runtime::adapters::SERVICE_WARMUP_VARIABLE.to_vec(),
            b"0".to_vec(),
        );
    }
    // The option wins over the project's file; with neither, a variable
    // the environment already holds stays as it is.
    let warmup = if options.warmup.is_empty() {
        &settings.warmup
    } else {
        &options.warmup
    };
    if !warmup.is_empty() {
        loaded.environment.insert(
            phera_runtime::adapters::WARMUP_REQUESTS_VARIABLE.to_vec(),
            warmup.join(" ").into_bytes(),
        );
    }
    site.exclude_files(loaded.paths);
    let site = Arc::new(site);
    let trusted_proxies = match options
        .trusted_proxies
        .as_deref()
        .map(TrustedProxies::parse)
    {
        Some(Ok(proxies)) => proxies,
        Some(Err(error)) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
        None => TrustedProxies::default(),
    };
    let listen = options.listen.unwrap_or_else(default_address);
    if options.debug_boot && (!options.debug || !options.dev) {
        eprintln!("--debug-boot requires --debug --dev");
        return ExitCode::FAILURE;
    }
    let mut debug = match crate::debug_settings::configure(
        options.debug,
        &settings.table,
        &options.ini,
        &loaded.environment,
    ) {
        Ok(debug) => debug,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(debug) = &mut debug {
        debug.boot = options.debug_boot;
        if !listen.ip().is_loopback() && !options.allow_remote_debug {
            eprintln!("Debugging on a non-loopback address requires --allow-remote-debug");
            return ExitCode::FAILURE;
        }
        eprintln!(
            "WARNING: PHP step debugging is enabled. Debug sessions connect to {}:{}.",
            debug.host, debug.port,
        );
    }
    let mut config = ServerConfig {
        addr: listen,
        ..ServerConfig::default()
    };
    if let Some(workers) = options.workers {
        config.workers = workers;
    }
    if let Some(request_timeout) = options.request_timeout {
        config.request_timeout = request_timeout;
    }
    if let Some(request_read_timeout) = options.request_read_timeout {
        config.request_read_timeout = request_read_timeout;
    }
    let workers = config.workers;
    let debug_server = debug.as_ref().map(|_| DebugServer {
        timeout: config.request_timeout.take(),
    });

    let mut runtime = Runtime::with_environment(loaded.environment)
        .with_startup_ini(options.ini)
        .with_web_development(options.dev)
        .without_web_processes(options.no_process)
        .without_class_warmup(!options.class_warmup);
    if let Some(debug) = debug {
        runtime = runtime.with_debug(debug);
    }
    if let (Adapter::Laravel, Some(front_controller)) = (adapter, site.front_controller()) {
        runtime = runtime.with_entry_script(
            front_controller.to_path_buf(),
            phera_runtime::adapters::LARAVEL_ENTRY,
        );
    }
    let preload = if options.preload || settings.preload {
        match runtime.preload_composer(&project) {
            Ok(summary) => {
                format!(
                    ", preloaded {} Composer files in {} ms ({} skipped)",
                    summary.compiled,
                    summary.elapsed.as_millis(),
                    summary.skipped
                )
            }
            Err(error) => {
                eprintln!("Cannot preload Composer files: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        String::new()
    };
    let factory = SiteHandlerFactory {
        runtime,
        adapter,
        debug_server,
        site: site.clone(),
        compress: options.compress,
        trusted_proxies: Arc::new(trusted_proxies),
        health_path: options.health_path.map(Arc::from),
    };
    let server = match Server::bind(config, factory) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("Cannot listen on {listen}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Phera is serving {} on http://{} with {workers} worker threads{preload}",
        site.describe(),
        server.local_addr()
    );
    let result = server.run();
    phera_runtime::flush_web_log();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("The server stopped: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Which framework adapter `phera serve` uses (see `phera_runtime::adapters`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Adapter {
    None,
    /// The front controller runs Phera's Laravel entry script.
    Laravel,
    /// `APP_RUNTIME` names Phera's runtime for symfony/runtime.
    SymfonyRuntime,
}

impl Adapter {
    /// The adapter for the site, setting `APP_RUNTIME` for a symfony/runtime
    /// application that has not chosen a runtime itself.
    fn detect(site: &Site, environment: &mut phera_dotenv::Environment) -> Adapter {
        let root = site.document_root();
        let front_controller = site.front_controller();
        if front_controller.is_some_and(|script| script == root.join("index.php"))
            && phera_runtime::adapters::is_laravel(root)
        {
            Adapter::Laravel
        } else if phera_runtime::adapters::uses_symfony_runtime(root)
            && environment.get(b"APP_RUNTIME").is_none()
        {
            environment.insert(
                b"APP_RUNTIME".to_vec(),
                phera_runtime::adapters::SYMFONY_RUNTIME_CLASS
                    .as_bytes()
                    .to_vec(),
            );
            Adapter::SymfonyRuntime
        } else {
            Adapter::None
        }
    }
}

/// Where to listen when `--listen` is not given. A hosting platform says
/// which port it will send traffic to through `PORT`, and reaches the
/// container from outside, so every interface is listened on. Without
/// `PORT` this is a developer's machine: the local interface only.
fn default_address() -> SocketAddr {
    match std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
    {
        Some(port) => SocketAddr::from(([0, 0, 0, 0], port)),
        None => SocketAddr::from(([127, 0, 0, 1], 8080)),
    }
}

/// Creates one handler per worker thread. The runtime and the site are
/// shared; everything a request touches while running is created per
/// request, on the worker's own thread.
struct SiteHandlerFactory {
    runtime: Runtime,
    /// The framework adapter in use, which makes the front controller boot
    /// at start.
    adapter: Adapter,
    debug_server: Option<DebugServer>,
    site: Arc<Site>,
    compress: bool,
    trusted_proxies: Arc<TrustedProxies>,
    health_path: Option<Arc<str>>,
}

impl HandlerFactory for SiteHandlerFactory {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler> {
        let site = SiteHandler {
            runtime: self
                .runtime
                .web_worker(worker_index)
                .with_background_boots(),
            adapter: self.adapter,
            site: self.site.clone(),
            compress: self.compress,
            trusted_proxies: self.trusted_proxies.clone(),
            health_path: self.health_path.clone(),
        };
        match self.debug_server {
            Some(debug) => Rc::new(DebugSiteHandler {
                site,
                timeout: debug.timeout,
            }),
            None => Rc::new(site),
        }
    }
}

#[derive(Clone, Copy)]
struct DebugServer {
    timeout: Option<Duration>,
}

struct SiteHandler {
    runtime: WebWorker,
    adapter: Adapter,
    site: Arc<Site>,
    /// `--compress`: compress responses for clients that accept it.
    compress: bool,
    /// `--trusted-proxies`: whose forwarding headers are believed.
    trusted_proxies: Arc<TrustedProxies>,
    /// `--health`: a path answered without running PHP.
    health_path: Option<Arc<str>>,
}

impl Handler for SiteHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(self.respond::<false>(request, None))
    }

    fn start(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(self.boot())
    }
}

/// Installed by the worker only when debugger startup is enabled.
struct DebugSiteHandler {
    site: SiteHandler,
    timeout: Option<Duration>,
}

impl Handler for DebugSiteHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        match self.timeout {
            None => Box::pin(self.site.respond::<true>(request, None)),
            Some(timeout) => Box::pin(async move {
                let clock = DebugTimeLimit::new(timeout);
                clock
                    .run(self.site.respond::<true>(request, Some(clock.clone())))
                    .await
                    .unwrap_or_else(|| Response::text(504, "the request took too long\n"))
            }),
        }
    }

    fn start(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(self.site.boot())
    }
}

impl SiteHandler {
    /// Boots the front controller before the first connection when it is
    /// known to call `Phera\snapshot()` (see `phera_runtime::BootReport`).
    async fn boot(&self) {
        let Some(script) = self.site.front_controller() else {
            return;
        };
        if self.adapter == Adapter::None
            && !self.runtime.debug_boot_enabled()
            && !phera_runtime::snapshot_remembered(script)
        {
            return;
        }
        let worker = self.runtime.index();
        match self.runtime.boot(script, self.site.document_root()).await {
            phera_runtime::BootReport::Snapshot { elapsed, files } => println!(
                "Worker {worker} booted {} in {} ms ({files} files); requests continue after Phera\\snapshot()",
                script.display(),
                elapsed.as_millis()
            ),
            phera_runtime::BootReport::Classic => println!(
                "Worker {worker}: {} ended without calling Phera\\snapshot(); it runs from the top for every request",
                script.display()
            ),
            phera_runtime::BootReport::Failed(message) => eprintln!(
                "Worker {worker}: DEFECT: {} cannot be snapshotted, its requests run from the top until a boot succeeds: {message}",
                script.display()
            ),
        }
    }

    async fn respond<const DEBUG: bool>(
        &self,
        request: Request,
        time_limit: Option<DebugTimeLimit>,
    ) -> Response {
        // A platform asks "are you alive?" every few seconds. The answer
        // must not depend on the application, its database or a free worker
        // slot in PHP.
        if self.health_path.as_deref() == Some(request.path.as_str()) {
            // How many requests ran from the top because a snapshot could
            // not be kept or copied: each one is a defect to fix.
            let from_the_top = phera_runtime::requests_from_the_top().to_string();
            return Response::text(200, "ok\n")
                .with_header("Cache-Control", "no-store")
                .with_header("Phera-From-The-Top", from_the_top);
        }
        let site = self.site.clone();
        let path = request.path.clone();
        let conditions = Conditions::of(&request, self.compress);
        let accept_encoding = request
            .header("accept-encoding")
            .filter(|_| self.compress)
            .map(|value| String::from_utf8_lossy(value).into_owned());
        // Looking at the file system blocks, so it runs off the worker
        // thread, which must stay free for other requests.
        let found = tokio::task::spawn_blocking(move || match site.resolve(&path) {
            Resolved::Script(script) => Found::Script(script),
            Resolved::File(file) => Found::Answer(static_file::answer(&site, &file, &conditions)),
            Resolved::NotFound => Found::Answer(Response::text(404, "Not Found\n")),
        })
        .await;
        match found {
            Ok(Found::Script(script)) => {
                let request = to_runtime(request, &self.trusted_proxies);
                let root = self.site.document_root();
                let response = if DEBUG {
                    self.runtime
                        .handle_debug_request(&script, root, request, time_limit)
                        .await
                } else {
                    self.runtime.handle_request(&script, root, request).await
                };
                let response = from_runtime(response);
                if self.compress {
                    compress::php_response(response, accept_encoding.as_deref()).await
                } else {
                    response
                }
            }
            Ok(Found::Answer(response)) => response,
            Err(_) => Response::text(500, "Internal Server Error\n"),
        }
    }
}

/// What looking up a request path on disk led to.
enum Found {
    /// A PHP file to run.
    Script(std::path::PathBuf),
    /// A response that is ready: a static file, or "not found".
    Answer(Response),
}

/// Hands a request to the runtime. When it came through a trusted proxy,
/// the address, scheme, host and port are those of the original request.
fn to_runtime(mut request: Request, trusted_proxies: &TrustedProxies) -> HttpRequest {
    let original = trusted_proxies
        .original_request(request.remote_addr.ip(), &request.headers)
        .unwrap_or_default();
    if let Some(host) = &original.host {
        request
            .headers
            .retain(|(name, _)| !name.eq_ignore_ascii_case("host"));
        request
            .headers
            .insert(0, ("host".to_string(), host.as_bytes().to_vec()));
    }
    let mut local_addr = request.local_addr;
    if let Some(port) = original.server_port() {
        local_addr.set_port(port);
    }
    HttpRequest {
        method: request.method,
        target: request.target,
        path: request.path,
        query: request.query,
        protocol: match request.version {
            HttpVersion::Http10 => "HTTP/1.0",
            HttpVersion::Http11 => "HTTP/1.1",
        }
        .to_string(),
        headers: request.headers,
        body: request.body,
        remote_addr: original.client_address().unwrap_or(request.remote_addr),
        local_addr,
        https: original.https.unwrap_or(false),
    }
}

fn from_runtime(response: HttpResponse) -> Response {
    let mut converted = Response::new(response.status).with_body(response.body);
    for (name, value) in response.headers {
        converted = converted.with_header(name, value);
    }
    converted
}
