//! One worker at work: connections on one side, the worker's VM on the
//! other, and a queue in between.
//!
//! The connection code (`phera-server`) runs every connection as a task on
//! the worker's `LocalSet`. Each request is routed right there; one that a
//! route or the fallback answers becomes a [`Job`] in the worker's queue.
//! The `listen()` call itself (it holds the VM) takes the jobs one at a
//! time, calls the handler and sends the response back to the connection.
//! So requests never wait for a VM to be made, and PHP never runs two
//! handlers at once on one VM.

use std::future::Future;
use std::net::TcpListener as StdListener;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use phera_extension_api::{Context, NativeError, NativeResult, Value};
use phera_server::{Handler, Request, Response, ServerConfig};
use tokio::sync::{mpsc, oneshot, watch};

use super::request::{self, RequestData};
use super::response;
use super::router::{Lookup, Method, ParamRanges, RouteId, Router};
use super::worker::{ScriptLaunch, spawn_worker};

/// The routes of a listening server, shared by its connections and its VM.
pub(super) struct RouteTable {
    pub(super) router: Router,
    /// The handler of each route, at its `RouteId`.
    pub(super) handlers: Vec<Value>,
    pub(super) fallback: Option<Value>,
}

/// Everything `listen()` takes out of the server object.
pub(super) struct WorkerSetup {
    pub(super) index: usize,
    pub(super) listener: StdListener,
    pub(super) routes: Rc<RouteTable>,
    pub(super) worker_start: Vec<Value>,
    pub(super) stop: Arc<watch::Sender<bool>>,
    pub(super) workers: usize,
}

/// What answers a queued request.
pub(super) enum Target {
    Route { route: RouteId, params: ParamRanges },
    Fallback,
}

struct Job {
    request: Request,
    target: Target,
    reply: oneshot::Sender<Response>,
}

/// The `phera-server` handler of a worker: routes, then queues for the VM.
struct QueueHandler {
    routes: Rc<RouteTable>,
    jobs: mpsc::UnboundedSender<Job>,
}

impl Handler for QueueHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        let target = match route(&self.routes, &request) {
            Ok(target) => target,
            Err(response) => return Box::pin(std::future::ready(response)),
        };
        let (reply, response) = oneshot::channel();
        let job = Job {
            request,
            target,
            reply,
        };
        if self.jobs.send(job).is_err() {
            return Box::pin(std::future::ready(Response::text(
                503,
                "Service Unavailable",
            )));
        }
        Box::pin(async move {
            // The VM drops the job without an answer only when it stopped.
            response
                .await
                .unwrap_or_else(|_| Response::text(503, "Service Unavailable"))
        })
    }
}

/// The route or fallback for `request`, or the answer when there is none.
fn route(routes: &RouteTable, request: &Request) -> Result<Target, Response> {
    let lookup = match Method::from_request(&request.method) {
        Some(method) => routes.router.lookup(method, &request.path),
        None => Lookup::NotFound,
    };
    match lookup {
        Lookup::Found { route, params } => Ok(Target::Route { route, params }),
        Lookup::MethodNotAllowed { allowed } => {
            Err(Response::text(405, "Method Not Allowed")
                .with_header("allow", allowed.allow_header()))
        }
        Lookup::NotFound if routes.fallback.is_some() => Ok(Target::Fallback),
        Lookup::NotFound if Method::from_request(&request.method).is_none() => {
            Err(Response::text(501, "Not Implemented"))
        }
        Lookup::NotFound => Err(Response::text(404, "Not Found")),
    }
}

/// `listen()` in the VM that ran the script: starts the other workers,
/// serves as worker 0, and waits for the others to stop.
pub(super) async fn serve_with_workers(
    context: &mut dyn Context,
    setup: WorkerSetup,
    launch: Option<ScriptLaunch>,
) -> NativeResult {
    phera_server::shutdown_on_signal(setup.stop.clone());
    let mut threads = Vec::new();
    if setup.workers > 1 {
        let Some(launch) = launch else {
            return Err(NativeError::throw(
                "LogicException",
                "Phera\\Http\\Server: more than one worker needs a script run by the phera command",
            ));
        };
        for index in 1..setup.workers {
            match start_worker(&launch, index, &setup) {
                Ok(thread) => threads.push(thread),
                Err(error) => {
                    setup.stop.send_replace(true);
                    join(threads);
                    return Err(error);
                }
            }
        }
    }
    let stop = setup.stop.clone();
    let result = serve(context, setup).await;
    // However worker 0 ended, the others end too.
    stop.send_replace(true);
    join(threads);
    result
}

fn start_worker(
    launch: &ScriptLaunch,
    index: usize,
    setup: &WorkerSetup,
) -> Result<JoinHandle<()>, NativeError> {
    let fail = |error: std::io::Error| {
        NativeError::throw(
            "RuntimeException",
            format!("Phera\\Http\\Server: cannot start worker {index}: {error}"),
        )
    };
    let listener = phera_server::listener_for_next_worker(&setup.listener).map_err(fail)?;
    spawn_worker(launch.clone(), index, listener, setup.stop.clone()).map_err(fail)
}

fn join(threads: Vec<JoinHandle<()>>) {
    for thread in threads {
        if thread.join().is_err() {
            eprintln!("phera: an HTTP worker thread panicked");
        }
    }
}

/// Serves as one worker until the server stops: runs the `onWorkerStart`
/// callbacks, then answers the queued requests on this VM.
pub(super) async fn serve(context: &mut dyn Context, setup: WorkerSetup) -> NativeResult {
    let WorkerSetup {
        index,
        listener,
        routes,
        worker_start,
        stop,
        ..
    } = setup;
    let shutdown = stop.subscribe();
    for callback in &worker_start {
        context
            .call(callback, vec![Value::Int(index as i64)])
            .await?;
    }
    drop(worker_start);

    let (jobs, mut queue) = mpsc::unbounded_channel();
    let handler = Rc::new(QueueHandler {
        routes: routes.clone(),
        jobs,
    });
    let config = ServerConfig {
        workers: 1,
        ..ServerConfig::default()
    };
    let mut serving = tokio::task::spawn_local(phera_server::serve_local(
        index, listener, handler, config, shutdown,
    ));

    let mut outcome = Ok(Value::Null);
    let mut served = false;
    loop {
        tokio::select! {
            job = queue.recv() => {
                let Some(job) = job else { break };
                if let Err(exit) = answer(context, &routes, job).await {
                    // `exit()` in a handler ends the program, as it would
                    // any PHP script: every worker stops.
                    stop.send_replace(true);
                    outcome = Err(exit);
                    break;
                }
            }
            _ = &mut serving => {
                served = true;
                break;
            }
        }
    }
    // Requests still queued are answered 503 by their connections.
    drop(queue);
    if !served {
        let _ = serving.await;
    }
    outcome
}

/// Runs one job's handler and sends its response. `Err` only for `exit()`.
async fn answer(
    context: &mut dyn Context,
    routes: &Rc<RouteTable>,
    job: Job,
) -> Result<(), NativeError> {
    let Job {
        request,
        target,
        reply,
    } = job;
    let (handler, route, params) = match target {
        Target::Route { route, params } => (&routes.handlers[route as usize], Some(route), params),
        Target::Fallback => match &routes.fallback {
            Some(fallback) => (fallback, None, ParamRanges::new()),
            None => {
                let _ = reply.send(Response::text(404, "Not Found"));
                return Ok(());
            }
        },
    };
    let data = RequestData::new(request, routes.clone(), route, params);
    let outcome = run_handler(context, handler, data).await;
    let response = match outcome {
        Ok(response) => response,
        Err(error) => {
            context.report_unhandled(error)?;
            Response::text(500, "Internal Server Error")
        }
    };
    // For HEAD the connection code sends the headers of this response (its
    // length included) and leaves the body out.
    let _ = reply.send(response);
    Ok(())
}

async fn run_handler(
    context: &mut dyn Context,
    handler: &Value,
    data: RequestData,
) -> Result<Response, NativeError> {
    let request = request::new_object(context, data)?;
    let result = context.call(handler, vec![request]).await?;
    response::from_handler_result(context, result).await
}
