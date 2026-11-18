//! `phera serve`: answers HTTP requests with PHP scripts and static files.
//!
//! This is the glue between two crates that do not know each other: the
//! HTTP server, which only knows a `Handler`, and the runtime, which only
//! knows how to run a script for a request.

mod site;

use std::future::Future;
use std::pin::Pin;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use phera_runtime::{HttpRequest, HttpResponse, Runtime};
use phera_server::{Handler, HandlerFactory, HttpVersion, Request, Response, Server, ServerConfig};

use crate::arguments::ServeOptions;
use site::{Resolved, Site};

pub(crate) fn serve(options: ServeOptions) -> ExitCode {
    let mut site = match Site::open(&options.path) {
        Ok(site) => site,
        Err(error) => {
            eprintln!("Cannot serve {}: {error}", options.path.display());
            return ExitCode::FAILURE;
        }
    };
    let root = site.document_root();
    let project = if root.join("composer.json").is_file() {
        root
    } else {
        root.parent()
            .filter(|p| p.join("composer.json").is_file())
            .unwrap_or(root)
    };
    let loaded = match options
        .files
        .load(project, phera_dotenv::Environment::process())
    {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    site.exclude_files(loaded.paths);
    let site = Arc::new(site);
    let mut config = ServerConfig {
        addr: options.listen,
        ..ServerConfig::default()
    };
    if let Some(workers) = options.workers {
        config.workers = workers;
    }
    let workers = config.workers;

    let factory = SiteHandlerFactory {
        runtime: Runtime::with_environment(loaded.environment),
        site: site.clone(),
    };
    let server = match Server::bind(config, factory) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("Cannot listen on {}: {error}", options.listen);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Phera is serving {} on http://{} with {workers} worker threads",
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

/// Creates one handler per worker thread. The runtime and the site are
/// shared; everything a request touches while running is created per
/// request, on the worker's own thread.
struct SiteHandlerFactory {
    runtime: Runtime,
    site: Arc<Site>,
}

impl HandlerFactory for SiteHandlerFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(SiteHandler {
            runtime: self.runtime.clone(),
            site: self.site.clone(),
        })
    }
}

struct SiteHandler {
    runtime: Runtime,
    site: Arc<Site>,
}

impl Handler for SiteHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(self.respond(request))
    }
}

impl SiteHandler {
    async fn respond(&self, request: Request) -> Response {
        let site = self.site.clone();
        let path = request.path.clone();
        // Looking at the file system blocks, so it runs off the worker
        // thread, which must stay free for other requests.
        let resolved = tokio::task::spawn_blocking(move || site.resolve(&path)).await;
        match resolved {
            Ok(Resolved::Script(script)) => {
                let response = self
                    .runtime
                    .handle_request(&script, self.site.document_root(), to_runtime(request))
                    .await;
                from_runtime(response)
            }
            Ok(Resolved::File { content_type, body }) => Response::new(200)
                .with_header("Content-Type", content_type)
                .with_body(body),
            Ok(Resolved::NotFound) => Response::text(404, "Not Found\n"),
            Err(_) => Response::text(500, "Internal Server Error\n"),
        }
    }
}

fn to_runtime(request: Request) -> HttpRequest {
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
        remote_addr: request.remote_addr,
        local_addr: request.local_addr,
    }
}

fn from_runtime(response: HttpResponse) -> Response {
    let mut converted = Response::new(response.status).with_body(response.body);
    for (name, value) in response.headers {
        converted = converted.with_header(name, value);
    }
    converted
}
