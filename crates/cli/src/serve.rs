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

use phera_runtime::{HttpRequest, HttpResponse, Runtime, WebWorker};
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

    let runtime = Runtime::with_environment(loaded.environment)
        .with_startup_ini(options.ini)
        .with_web_development(options.dev)
        .without_web_processes(options.no_process);
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
    match server.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("The server stopped: {error}");
            ExitCode::FAILURE
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
    site: Arc<Site>,
    compress: bool,
    trusted_proxies: Arc<TrustedProxies>,
    health_path: Option<Arc<str>>,
}

impl HandlerFactory for SiteHandlerFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(SiteHandler {
            runtime: self.runtime.web_worker(),
            site: self.site.clone(),
            compress: self.compress,
            trusted_proxies: self.trusted_proxies.clone(),
            health_path: self.health_path.clone(),
        })
    }
}

struct SiteHandler {
    runtime: WebWorker,
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
        Box::pin(self.respond(request))
    }
}

impl SiteHandler {
    async fn respond(&self, request: Request) -> Response {
        // A platform asks "are you alive?" every few seconds. The answer
        // must not depend on the application, its database or a free worker
        // slot in PHP.
        if self.health_path.as_deref() == Some(request.path.as_str()) {
            return Response::text(200, "ok\n").with_header("Cache-Control", "no-store");
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
                let response = self
                    .runtime
                    .handle_request(
                        &script,
                        self.site.document_root(),
                        to_runtime(request, &self.trusted_proxies),
                    )
                    .await;
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
