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
mod web_output;

use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use phera_runtime::{
    DebugTimeLimit, HttpRequest, HttpResponse, RouteBootReport, RouteDispatch, Runtime,
    WebSocketEvent as RuntimeWebSocketEvent, WebSocketSink, WebWorker,
};
use phera_server::{
    Handler, HandlerFactory, HttpVersion, Request, Response, ResponseWriter, Server, ServerConfig,
    WebSocketEvent, WebSocketEventKind, WebSocketHub, WebSocketOutcome, WebSocketUpgrade,
};

use crate::arguments::ServeOptions;
use forwarded::TrustedProxies;
use site::{Resolved, Site};
use static_file::Conditions;

pub(crate) fn serve(options: ServeOptions) -> ExitCode {
    let image = if let Some(image_path) = &options.image {
        if options.dev {
            eprintln!(
                "phera serve: --dev ignores --image {} and compiles from source",
                image_path.display()
            );
            None
        } else {
            match phera_runtime::image::open_image(image_path) {
                Ok(image) => {
                    if image.mode() == phera_bytecode::facts::BuildMode::Strict
                        && !image.manifest().has_web
                    {
                        eprintln!(
                            "Image {} has no web units; rebuild with `phera build --target web` or `--target both`.",
                            image_path.display()
                        );
                        return ExitCode::FAILURE;
                    }
                    Some(image)
                }
                Err(error) => {
                    eprintln!("Cannot open image {}: {error}", image_path.display());
                    return ExitCode::FAILURE;
                }
            }
        }
    } else {
        None
    };

    let mut site = match &image {
        Some(image) => match Site::open_with_image(&options.path, image.clone()) {
            Ok(site) => site,
            Err(error) => {
                eprintln!("Cannot serve {}: {error}", options.path.display());
                return ExitCode::FAILURE;
            }
        },
        None => match Site::open(&options.path) {
            Ok(site) => site,
            Err(error) => {
                eprintln!("Cannot serve {}: {error}", options.path.display());
                return ExitCode::FAILURE;
            }
        },
    };
    let project = match &image {
        Some(image) => {
            let detected = config::project_root(site.document_root()).to_path_buf();
            if let Err(error) = image.verify_serve_root(site.document_root(), &detected) {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
            PathBuf::from(&image.manifest().project_root)
        }
        None => config::project_root(site.document_root()).to_path_buf(),
    };
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
    if let Some(image_path) = &options.image {
        loaded.paths.push(
            image_path
                .canonicalize()
                .unwrap_or_else(|_| image_path.clone()),
        );
    }
    let adapter = Adapter::detect(&site, image.as_deref(), &mut loaded.environment);
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
    let explicit_routes = match options.routes.clone().or(settings.routes) {
        Some(path) => match path.canonicalize() {
            Ok(canonical) if canonical.is_file() => Some(canonical),
            Ok(_) => {
                eprintln!("Cannot load routes from {}: not a file", path.display());
                return ExitCode::FAILURE;
            }
            Err(error) => {
                let from_image = image.as_ref().and_then(|img| {
                    img.find_canonical(&path)
                        .or_else(|| path.to_str().and_then(|rel| img.find_relative(rel)))
                        .filter(|entry| entry.unit_web.is_some())
                        .map(|entry| PathBuf::from(&entry.canonical_path))
                });
                if let Some(canonical) = from_image {
                    Some(canonical)
                } else {
                    eprintln!("Cannot load routes from {}: {error}", path.display());
                    return ExitCode::FAILURE;
                }
            }
        },
        None => None,
    };
    if let Some(routes_file) = &explicit_routes {
        loaded.paths.push(routes_file.clone());
    }
    site.exclude_files(loaded.paths);
    let site = Arc::new(site);
    let route_source = if let Some(script) = explicit_routes {
        Some(RouteSource {
            script: Arc::from(script),
            hybrid: true,
            explicit: true,
        })
    } else if site.is_single_script() {
        site.front_controller().map(|script| RouteSource {
            script: Arc::from(script.to_path_buf()),
            hybrid: false,
            explicit: false,
        })
    } else {
        None
    };
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
        if image
            .as_ref()
            .is_some_and(|img| img.mode() == phera_bytecode::facts::BuildMode::Strict)
        {
            eprintln!(
                "Cannot enable PHP step debugging with a strict AOT image (use a hybrid image or omit --image)."
            );
            return ExitCode::FAILURE;
        }
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
    if let Some(max_connections) = options.max_connections.or(settings.max_connections) {
        config.max_connections_per_worker = max_connections;
    }
    if let Some(backlog) = options.backlog.or(settings.backlog) {
        config.backlog = backlog;
    }
    if let Some(idle_timeout) = options.idle_timeout.or(settings.idle_timeout) {
        config.idle_timeout = idle_timeout;
    }
    if let Some(keepalive_shed_at) = options.keepalive_shed_at.or(settings.keepalive_shed_at) {
        config.keepalive_shed_at = Some(keepalive_shed_at);
    }
    if let Some(request_timeout) = options.request_timeout {
        config.request_timeout = request_timeout;
    }
    if let Some(request_read_timeout) = options.request_read_timeout {
        config.request_read_timeout = request_read_timeout;
    }
    apply_websocket_config(&mut config, &settings.websocket, &options);
    let workers = config.workers;
    let debug_server = debug.as_ref().map(|_| DebugServer {
        timeout: config.request_timeout.take(),
    });
    let ws_hub = Arc::new(WebSocketHub::new(
        config.websocket.send_queue_len,
        config.websocket.send_queue_bytes,
    ));

    let mut runtime = Runtime::with_environment(loaded.environment)
        .with_startup_ini(options.ini)
        .with_web_development(options.dev)
        .without_web_processes(options.no_process)
        .without_class_warmup(!options.class_warmup)
        .with_websocket_sink(Arc::new(HubSink(ws_hub.clone())));
    if let Some(image) = &image {
        runtime = runtime.with_image(image.clone());
    }
    if let Some(limit) = options.max_active_requests.or(settings.max_active_requests) {
        runtime = runtime.with_max_concurrent_requests_per_worker(Some(limit));
    }
    if let Some(debug) = debug {
        runtime = runtime.with_debug(debug);
    }
    if let (Adapter::Laravel, Some(front_controller)) = (adapter, site.front_controller()) {
        runtime = runtime.with_entry_script(
            front_controller.to_path_buf(),
            phera_runtime::adapters::LARAVEL_ENTRY,
        );
    }
    let preload = if let Some(image) = &image {
        format!(
            ", image {} ({} files, {} mode)",
            options
                .image
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            image.files().len(),
            match image.mode() {
                phera_bytecode::facts::BuildMode::Hybrid => "hybrid",
                phera_bytecode::facts::BuildMode::Strict => "strict",
            }
        )
    } else if options.preload || settings.preload {
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
        route_source,
        compress: options.compress,
        trusted_proxies: Arc::new(trusted_proxies),
        health_path: options.health_path.map(Arc::from),
    };
    let server = match Server::bind_with_hub(config, factory, ws_hub) {
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
    fn detect(
        site: &Site,
        image: Option<&phera_runtime::AotImage>,
        environment: &mut phera_dotenv::Environment,
    ) -> Adapter {
        let root = site.document_root();
        let front_controller = site.front_controller();
        let image_framework = image.and_then(|img| img.manifest().framework.as_deref());
        let is_index_fc = front_controller.is_some_and(|script| script == root.join("index.php"));
        let image_laravel = image.is_some_and(|img| {
            img.manifest().framework.as_deref() == Some("laravel")
                && root == Path::new(&img.manifest().project_root).join("public")
        });
        if is_index_fc && (phera_runtime::adapters::is_laravel(root) || image_laravel) {
            Adapter::Laravel
        } else if (phera_runtime::adapters::uses_symfony_runtime(root)
            || (image_framework == Some("symfony")
                && image
                    .is_some_and(|img| img.find_relative("vendor/autoload_runtime.php").is_some())))
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

#[derive(Clone)]
struct RouteSource {
    script: Arc<Path>,
    hybrid: bool,
    explicit: bool,
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
    route_source: Option<RouteSource>,
    compress: bool,
    trusted_proxies: Arc<TrustedProxies>,
    health_path: Option<Arc<str>>,
}

impl HandlerFactory for SiteHandlerFactory {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler> {
        let compress = self.compress;
        let site = SiteHandler {
            runtime: self
                .runtime
                .web_worker(worker_index)
                .with_background_boots()
                .with_web_output(move |encoding| {
                    let writer = ResponseWriter::take_current()?;
                    let encoding = encoding
                        .filter(|_| compress)
                        .map(|value| String::from_utf8_lossy(value).into_owned());
                    Some(web_output::HttpOutput::new(writer, compress, encoding))
                }),
            adapter: self.adapter,
            site: self.site.clone(),
            route_source: self.route_source.clone(),
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
    route_source: Option<RouteSource>,
    /// `--compress`: compress responses for clients that accept it.
    compress: bool,
    /// `--trusted-proxies`: whose forwarding headers are believed.
    trusted_proxies: Arc<TrustedProxies>,
    /// `--health`: a path answered without running PHP.
    health_path: Option<Arc<str>>,
}

impl Handler for SiteHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        if self.health_path.as_deref() == Some(request.path.as_str()) {
            return Box::pin(std::future::ready(health_response()));
        }
        if self.runtime.router_handles(&request.method, &request.path) {
            return Box::pin(self.respond_router(request));
        }
        Box::pin(self.respond_classic::<false>(request, None))
    }

    fn handle_owned(self: Rc<Self>, request: Request) -> Pin<Box<dyn Future<Output = Response>>> {
        if self.health_path.as_deref() == Some(request.path.as_str()) {
            return Box::pin(std::future::ready(health_response()));
        }
        if self.runtime.router_handles(&request.method, &request.path) {
            return Box::pin(async move { self.respond_router(request).await });
        }
        Box::pin(async move { self.respond_classic::<false>(request, None).await })
    }

    fn start(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(self.boot())
    }

    fn handle_websocket(
        &self,
        event: WebSocketEvent,
    ) -> Pin<Box<dyn Future<Output = WebSocketOutcome> + '_>> {
        Box::pin(async move {
            let kind = match event.kind {
                WebSocketEventKind::Open => RuntimeWebSocketEvent::Open,
                WebSocketEventKind::Message { data, binary } => {
                    RuntimeWebSocketEvent::Message { data, binary }
                }
                WebSocketEventKind::Close { code, reason } => {
                    RuntimeWebSocketEvent::Close { code, reason }
                }
            };
            let outcome = self
                .runtime
                .handle_websocket_event(
                    Path::new(&*event.entry),
                    self.site.document_root(),
                    &event.connection_id,
                    &event.handler_class,
                    kind,
                )
                .await;
            WebSocketOutcome {
                close_1011: outcome.close_1011,
            }
        })
    }

    fn websocket_origin_allowed(
        &self,
        request: &Request,
        allowed_origins: Option<&[String]>,
    ) -> bool {
        let original = self
            .trusted_proxies
            .original_request(request.remote_addr.ip(), &request.headers)
            .unwrap_or_default();
        let effective_https = original.https.unwrap_or(false);
        let effective_host = original
            .host
            .as_deref()
            .map(str::as_bytes)
            .or_else(|| request.header("host"));
        phera_server::websocket::handshake::validate_origin(
            request.header("origin"),
            effective_https,
            effective_host,
            allowed_origins,
        )
    }
}

/// Installed by the worker only when debugger startup is enabled.
struct DebugSiteHandler {
    site: SiteHandler,
    timeout: Option<Duration>,
}

impl Handler for DebugSiteHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        if self.site.health_path.as_deref() == Some(request.path.as_str()) {
            return Box::pin(std::future::ready(health_response()));
        }
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

    fn handle_owned(self: Rc<Self>, request: Request) -> Pin<Box<dyn Future<Output = Response>>> {
        if self.site.health_path.as_deref() == Some(request.path.as_str()) {
            return Box::pin(std::future::ready(health_response()));
        }
        match self.timeout {
            None => Box::pin(async move { self.site.respond::<true>(request, None).await }),
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

    fn handle_websocket(
        &self,
        event: WebSocketEvent,
    ) -> Pin<Box<dyn Future<Output = WebSocketOutcome> + '_>> {
        self.site.handle_websocket(event)
    }

    fn websocket_origin_allowed(
        &self,
        request: &Request,
        allowed_origins: Option<&[String]>,
    ) -> bool {
        self.site.websocket_origin_allowed(request, allowed_origins)
    }
}

impl SiteHandler {
    /// Boots a `Phera\Http\Router` (if configured or returned by a single-file
    /// entry script) and/or a `Phera\snapshot()` front controller before the
    /// first connection.
    async fn boot(&self) {
        let worker = self.runtime.index();
        if let Some(source) = &self.route_source {
            match self
                .runtime
                .boot_router(
                    &source.script,
                    self.site.document_root(),
                    source.hybrid,
                    source.explicit,
                )
                .await
            {
                RouteBootReport::Ready {
                    elapsed,
                    routes,
                    files,
                } => {
                    println!(
                        "Worker {worker} loaded {routes} routes from {} in {} ms ({files} files)",
                        source.script.display(),
                        elapsed.as_millis()
                    );
                    if !source.hybrid {
                        return;
                    }
                }
                RouteBootReport::Failed(message) => {
                    eprintln!(
                        "Worker {worker}: failed to boot routes from {}: {message}",
                        source.script.display()
                    );
                    if !source.hybrid {
                        return;
                    }
                }
                RouteBootReport::NotARouter => {
                    if source.explicit {
                        eprintln!(
                            "Worker {worker}: {} did not return a Phera\\Http\\Router",
                            source.script.display()
                        );
                    }
                }
            }
        }
        let Some(script) = self.site.front_controller() else {
            return;
        };
        if self.adapter == Adapter::None
            && !self.runtime.debug_boot_enabled()
            && !phera_runtime::snapshot_remembered(script)
        {
            return;
        }
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
        if self.health_path.as_deref() == Some(request.path.as_str()) {
            return health_response();
        }
        if self.runtime.router_handles(&request.method, &request.path) {
            return self.respond_router(request).await;
        }
        self.respond_classic::<DEBUG>(request, time_limit).await
    }

    /// Compact hot path for `Phera\Http\Router` requests: isolated from classic
    /// PHP (`handle_request`) and static-file futures so per-request allocation
    /// stays minimal.
    async fn respond_router(&self, request: Request) -> Response {
        let accept_encoding = if self.compress {
            request
                .header("accept-encoding")
                .map(|value| String::from_utf8_lossy(value).into_owned())
        } else {
            None
        };
        let rt_request = to_runtime(request, &self.trusted_proxies);
        match self.runtime.dispatch_router(rt_request).await {
            RouteDispatch::Handled(response) => {
                let response = from_runtime(response, self.site.document_root());
                if self.compress && response.upgrade.is_none() {
                    Box::pin(compress::php_response(response, accept_encoding.as_deref())).await
                } else {
                    response
                }
            }
            RouteDispatch::Fallthrough(_) => Response::text(404, "Not Found\n"),
        }
    }

    async fn respond_classic<const DEBUG: bool>(
        &self,
        request: Request,
        time_limit: Option<DebugTimeLimit>,
    ) -> Response {
        // Finding what the path names costs a `stat` or two of a file the
        // kernel has cached, so it runs here, on the worker thread, as in
        // nginx's event loop. Handing it to the blocking pool cost more
        // than the lookup: a mutex shared with the pool's threads, a
        // condition variable and two thread switches for every request.
        // Only reading a static file's bytes goes to the pool.
        let script = match self.site.resolve(&request.path) {
            Resolved::Script(script) => script,
            Resolved::File(file) => return self.answer_file(file, request).await,
            Resolved::NotFound => return Response::text(404, "Not Found\n"),
        };
        let accept_encoding = if self.compress {
            request
                .header("accept-encoding")
                .map(|value| String::from_utf8_lossy(value).into_owned())
        } else {
            None
        };
        let request = to_runtime(request, &self.trusted_proxies);
        let root = self.site.document_root();
        let response = if DEBUG {
            self.runtime
                .handle_debug_request(&script, root, request, time_limit)
                .await
        } else {
            self.runtime.handle_request(&script, root, request).await
        };
        let response = from_runtime(response, &script);
        if self.compress && response.upgrade.is_none() {
            compress::php_response(response, accept_encoding.as_deref()).await
        } else {
            response
        }
    }

    /// Answers a request for a static file of the site. Opening and reading
    /// the file may block, so it runs on the blocking pool.
    async fn answer_file(&self, file: std::path::PathBuf, request: Request) -> Response {
        let site = self.site.clone();
        let conditions = Conditions::of(&request, self.compress);
        match tokio::task::spawn_blocking(move || static_file::answer(&site, &file, &conditions))
            .await
        {
            Ok(response) => response,
            Err(_) => Response::text(500, "Internal Server Error\n"),
        }
    }
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
            HttpVersion::Http2 => "HTTP/2.0",
        }
        .to_string(),
        headers: request.headers,
        body: request.body,
        remote_addr: original.client_address().unwrap_or(request.remote_addr),
        local_addr,
        https: original.https.unwrap_or(false),
    }
}

/// A platform asks "are you alive?" every few seconds. The answer must not
/// depend on the application, its database or a free worker slot in PHP.
fn health_response() -> Response {
    // How many requests ran from the top because a snapshot could
    // not be kept or copied: each one is a defect to fix.
    let from_the_top = phera_runtime::requests_from_the_top().to_string();
    Response::text(200, "ok\n")
        .with_header("Cache-Control", "no-store")
        .with_header("Phera-From-The-Top", from_the_top)
}

fn from_runtime(response: HttpResponse, script: &Path) -> Response {
    let upgrade = response.websocket.map(|ws| {
        Box::new(WebSocketUpgrade {
            entry: script.to_string_lossy().into_owned(),
            handler_class: ws.handler,
            subprotocol: ws.subprotocol,
            context: ws.context,
            rooms: ws.rooms,
        })
    });
    Response {
        status: response.status,
        headers: response.headers,
        body: phera_server::Body::Bytes(response.body),
        upgrade,
    }
}

fn apply_websocket_config(
    config: &mut ServerConfig,
    file: &config::WebSocketSettings,
    options: &ServeOptions,
) {
    let ws = &mut config.websocket;
    if let Some(n) = options.ws_max_connections.or(file.max_connections) {
        ws.max_connections_per_worker = n;
    }
    if let Some(bytes) = options.ws_max_message_size.or(file.max_message_size) {
        ws.max_message_bytes = bytes;
    }
    if let Some(timeout) = options.ws_idle_timeout.or(file.idle_timeout) {
        ws.idle_timeout = timeout;
    }
    if let Some(interval) = options.ws_ping_interval.or(file.ping_interval) {
        ws.ping_interval = interval;
    }
    if let Some(queue) = options.ws_send_queue.or(file.send_queue) {
        ws.send_queue_len = queue;
    }
    if let Some(bytes) = file.send_queue_bytes {
        ws.send_queue_bytes = bytes;
    }
    if let Some(origins) = options
        .ws_allowed_origins
        .as_ref()
        .or(file.allowed_origins.as_ref())
    {
        ws.allowed_origins = Some(origins.clone());
    }
}

struct HubSink(Arc<WebSocketHub>);

impl WebSocketSink for HubSink {
    fn send(&self, connection_id: &str, data: &[u8], binary: bool) -> bool {
        self.0.send(connection_id, data, binary)
    }

    fn broadcast(
        &self,
        room: &str,
        data: &[u8],
        binary: bool,
        exclude_connection_id: Option<&str>,
    ) -> usize {
        self.0.broadcast(room, data, binary, exclude_connection_id)
    }

    fn join(&self, connection_id: &str, room: &str) -> bool {
        self.0.join(connection_id, room)
    }

    fn leave(&self, connection_id: &str, room: &str) -> bool {
        self.0.leave(connection_id, room)
    }

    fn rooms(&self, connection_id: &str) -> Vec<String> {
        self.0.rooms(connection_id)
    }

    fn room_count(&self, room: &str) -> usize {
        self.0.room_count(room)
    }

    fn connections(&self, room: &str) -> Vec<String> {
        self.0.connections(room)
    }

    fn close(&self, connection_id: &str, code: u16, reason: &str) -> bool {
        self.0.close(connection_id, code, reason)
    }

    fn context(&self, connection_id: &str) -> Option<Vec<u8>> {
        self.0.context(connection_id)
    }

    fn set_context(&self, connection_id: &str, context: Vec<u8>) -> bool {
        self.0.set_context(connection_id, context)
    }
}
