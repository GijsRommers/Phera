//! Per-worker persistent VM pool for `Phera\Http\Router`.
//!
//! Each `phera serve` worker boots the route script once, freezes a snapshot,
//! and keeps a local pool of warm VM instances (`idle: Vec<PooledVm>`):
//!
//! - Fast sync routes (`/plaintext`, `/json`) pop 1 VM, run in a single poll,
//!   and push it straight back: pool size stays 1, zero forks, zero channel hops.
//! - Concurrent async I/O routes (DB/Redis/gRPC/sleep) auto-fork additional
//!   persistent instances from the route snapshot (up to `active_requests`,
//!   default 32), running `onStart($worker)` once per instance, so requests
//!   multiplex M:N on the worker's `LocalSet`.
//! - An uncaught exception reports as PHP reports it, answers 500, and keeps
//!   the instance alive; `exit()` or fatal state discards that instance cleanly
//!   without stopping the worker.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use phera_ext_http::Sapi;
use phera_extension_api::{Context, Object, Value};
use phera_vm::{Snapshot, Vm};

use super::request::{self, RequestData};
use super::response;
use super::router::{Lookup, Method, ParamRanges, RouteId, Router};
use super::router_obj::{ActiveRouterSlot, BootRouterSlot, RouterData};
use crate::snapshots::FileStamps;
use crate::{HttpRequest, HttpResponse, WebWorker};

/// Result of booting a script as a `Phera\Http\Router`.
pub enum RouteBootReport {
    /// Booted and armed a persistent route pool on this worker.
    Ready {
        elapsed: Duration,
        routes: usize,
        files: usize,
    },
    /// The script did not return a `Phera\Http\Router` (or call `Phera\Http\serve`).
    NotARouter,
    /// The script failed while booting.
    Failed(String),
}

/// Outcome of attempting to dispatch an HTTP request against the worker's router.
pub enum RouteDispatch {
    /// Handled directly by the router (matched route, fallback, 405, or non-hybrid 404/501).
    Handled(HttpResponse),
    /// Hybrid mode: unknown path with no fallback — continue to static files & classic PHP.
    Fallthrough(HttpRequest),
}

struct PooledVm {
    vm: Vm,
    router_obj: Rc<Object>,
}

struct RoutePool {
    script: PathBuf,
    document_root: PathBuf,
    router: Rc<Router>,
    has_fallback: bool,
    hybrid: bool,
    /// `Some` when the booted VM froze cleanly into a forkable snapshot (the
    /// normal case, enabling up to `active_requests` concurrent M:N instances).
    /// `None` if boot opened an un-forkable resource directly instead of using
    /// `onStart()` — falls back safely to 1 persistent instance.
    snapshot: Option<Rc<Snapshot>>,
    idle: RefCell<Vec<PooledVm>>,
    /// Concurrency limiter for this pool (`active_requests` when forkable, or 1
    /// when `snapshot` is `None`).
    limit: Option<Rc<tokio::sync::Semaphore>>,
    stamps: RefCell<FileStamps>,
    worker_index: usize,
}

/// Per-worker holder for an optional `Phera\Http\Router` pool.
#[derive(Default)]
pub struct WorkerRouter {
    pool: RefCell<Option<Rc<RoutePool>>>,
}

impl WorkerRouter {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.pool.borrow().is_some()
    }

    /// Quick in-memory check whether `(method, path)` matches a route or
    /// should be answered directly by the router (avoiding `to_runtime` /
    /// `spawn_blocking` when a hybrid route does not match).
    #[inline]
    pub fn matches_in_memory(&self, method: &str, path: &str) -> bool {
        let borrow = self.pool.borrow();
        let Some(pool) = borrow.as_ref() else {
            return false;
        };
        if !pool.hybrid {
            return true;
        }
        if pool.has_fallback {
            return true;
        }
        match Method::from_request(method) {
            Some(m) => !matches!(pool.router.lookup(m, path), Lookup::NotFound),
            None => false,
        }
    }
}

enum Target {
    Route { route: RouteId, params: ParamRanges },
    Fallback,
}

impl WebWorker {
    /// Whether this worker has a booted `Phera\Http\Router`.
    #[inline]
    pub fn has_router(&self) -> bool {
        self.router.is_active()
    }

    /// Whether the worker's router will answer `(method, path)` (always `true`
    /// in standalone router mode; `true` only on match/405/fallback in hybrid mode).
    #[inline]
    pub fn router_handles(&self, method: &str, path: &str) -> bool {
        self.router.matches_in_memory(method, path)
    }

    /// Boots `script` looking for a returned `Phera\Http\Router` (or a
    /// `Phera\Http\serve($router, ...)` call). When `hybrid` is true, unknown
    /// paths without a `fallback()` fall through to static files / classic PHP.
    /// When `explicit` is false (auto-detecting a front-controller script),
    /// skips execution unless the compiled unit references `Phera\Http` so
    /// classic side-effecting scripts never execute during startup.
    pub async fn boot_router(
        &self,
        script: &Path,
        document_root: &Path,
        hybrid: bool,
        explicit: bool,
    ) -> RouteBootReport {
        let started = Instant::now();
        let code = Rc::new(crate::worker_code_cache::RequestCodeCache::new(
            self.code.clone(),
        ));
        let unit = match self.runtime.entry_unit(&code, script).await {
            Ok(unit) => unit,
            Err(error) => return RouteBootReport::Failed(error.to_string()),
        };
        if !explicit && !unit_references_http_router(&unit) {
            return RouteBootReport::NotARouter;
        }
        let mut vm = self
            .runtime
            .new_vm(&self.native, Sapi::Web, self.environment.clone());
        vm.set_local_loader(code);
        crate::http::define_boot_superglobals(
            &mut vm,
            script,
            document_root,
            self.environment.as_ref().clone(),
        );
        phera_ext_streams::set_request(
            vm.state_mut(),
            script.parent().map(Path::to_path_buf),
            Vec::new().into(),
        );
        vm.state_mut().get_or_default::<BootRouterSlot>().active = true;

        let router_obj = match vm.boot_returning(unit).await {
            Ok(value) => match value.deref().clone() {
                Value::Object(obj)
                    if obj
                        .native
                        .borrow()
                        .as_ref()
                        .is_some_and(|n| n.is::<RouterData>()) =>
                {
                    obj
                }
                _ => return RouteBootReport::NotARouter,
            },
            Err((_, true)) => {
                match vm
                    .state_mut()
                    .get_or_default::<BootRouterSlot>()
                    .captured
                    .take()
                {
                    Some(obj) => obj,
                    None => return RouteBootReport::NotARouter,
                }
            }
            Err((status, false)) => {
                let output = String::from_utf8_lossy(&vm.output).trim().to_owned();
                return RouteBootReport::Failed(if output.is_empty() {
                    format!("route script exited with status {status}")
                } else {
                    output
                });
            }
        };
        vm.state_mut().get_or_default::<BootRouterSlot>().active = false;
        vm.output.clear();

        let (router, route_count, has_fallback) = {
            let mut native = router_obj.native.borrow_mut();
            let Some(data) = native.as_mut().and_then(|n| n.downcast_mut::<RouterData>()) else {
                return RouteBootReport::NotARouter;
            };
            data.frozen = true;
            (
                Rc::clone(&data.router),
                data.handlers.len(),
                data.fallback.is_some(),
            )
        };

        let files = vm.included_files();
        let file_count = files.len();
        vm.state_mut().get_or_default::<ActiveRouterSlot>().0 = Some(router_obj.clone());

        let (snapshot, initial_vm, limit) = match vm.freeze_snapshot() {
            Ok(snapshot) => {
                let snapshot = Rc::new(snapshot);
                // Pre-warm 1 persistent instance so the first request pays zero fork latency.
                let initial = match fork_instance(&snapshot, self.index, &self.code).await {
                    Ok(instance) => Some(instance),
                    Err(error) => return RouteBootReport::Failed(error),
                };
                (Some(snapshot), initial, self.active_requests.clone())
            }
            Err(_) => {
                // Boot opened an un-forkable resource directly (instead of onStart):
                // keep this single booted VM alive safely with concurrency capped at 1.
                let mut instance = PooledVm { vm, router_obj };
                if let Err(error) = run_on_start(&mut instance, self.index).await {
                    return RouteBootReport::Failed(error);
                }
                (
                    None,
                    Some(instance),
                    Some(Rc::new(tokio::sync::Semaphore::new(1))),
                )
            }
        };

        let mut idle = Vec::with_capacity(4);
        if let Some(instance) = initial_vm {
            idle.push(instance);
        }

        let pool = Rc::new(RoutePool {
            script: script.to_path_buf(),
            document_root: document_root.to_path_buf(),
            router,
            has_fallback,
            hybrid,
            snapshot,
            idle: RefCell::new(idle),
            limit,
            stamps: RefCell::new(FileStamps::of(files)),
            worker_index: self.index,
        });
        *self.router.pool.borrow_mut() = Some(pool);

        RouteBootReport::Ready {
            elapsed: started.elapsed(),
            routes: route_count,
            files: file_count,
        }
    }

    /// Dispatches `request` directly against this worker's `Phera\Http\Router`
    /// pool with zero `spawn_blocking` and zero channel hops.
    pub async fn dispatch_router(&self, request: HttpRequest) -> RouteDispatch {
        let Some(pool) = self.router.pool.borrow().clone() else {
            return RouteDispatch::Fallthrough(request);
        };

        // Revalidate changed files under `--dev` / opcache timestamp checks.
        let stale = pool
            .stamps
            .borrow_mut()
            .changed(self.runtime.include_policy);
        if stale {
            let _ = self
                .boot_router(&pool.script, &pool.document_root, pool.hybrid, true)
                .await;
            let Some(refreshed) = self.router.pool.borrow().clone() else {
                return RouteDispatch::Fallthrough(request);
            };
            return self.dispatch_on_pool(refreshed, request).await;
        }

        self.dispatch_on_pool(pool, request).await
    }

    async fn dispatch_on_pool(&self, pool: Rc<RoutePool>, request: HttpRequest) -> RouteDispatch {
        let parsed_method = Method::from_request(&request.method);
        let lookup = match parsed_method {
            Some(method) => pool.router.lookup(method, &request.path),
            None => Lookup::NotFound,
        };
        let target = match lookup {
            Lookup::Found { route, params } => Target::Route { route, params },
            Lookup::MethodNotAllowed { allowed } => {
                let mut resp = HttpResponse::plain(405, "Method Not Allowed");
                resp.headers.push((
                    "allow".to_owned(),
                    allowed.allow_header().as_bytes().to_vec(),
                ));
                return RouteDispatch::Handled(resp);
            }
            Lookup::NotFound if pool.has_fallback => Target::Fallback,
            Lookup::NotFound if pool.hybrid => return RouteDispatch::Fallthrough(request),
            Lookup::NotFound if parsed_method.is_none() => {
                return RouteDispatch::Handled(HttpResponse::plain(501, "Not Implemented"));
            }
            Lookup::NotFound => {
                return RouteDispatch::Handled(HttpResponse::plain(404, "Not Found"));
            }
        };

        // Bound concurrent M:N instances; try_acquire fast-paths uncontended sync calls.
        let _permit = match &pool.limit {
            Some(sem) => match sem.try_acquire() {
                Ok(permit) => Some(permit),
                Err(_) => match sem.acquire().await {
                    Ok(permit) => Some(permit),
                    Err(_) => {
                        return RouteDispatch::Handled(HttpResponse::plain(
                            503,
                            "Service Unavailable",
                        ));
                    }
                },
            },
            None => None,
        };

        // Pop a warm persistent VM from the worker's idle stack, or fork one on demand
        // when all existing instances are concurrently parked at an async `.await`.
        let popped = pool.idle.borrow_mut().pop();
        let mut instance = match popped {
            Some(instance) => instance,
            None => match &pool.snapshot {
                Some(snapshot) => {
                    match fork_instance(snapshot, pool.worker_index, &self.code).await {
                        Ok(instance) => instance,
                        Err(error) => {
                            eprintln!(
                                "phera: worker {}: route fork failed: {error}",
                                pool.worker_index
                            );
                            return RouteDispatch::Handled(HttpResponse::plain(
                                500,
                                "Internal Server Error",
                            ));
                        }
                    }
                }
                None => {
                    return RouteDispatch::Handled(HttpResponse::plain(503, "Service Unavailable"));
                }
            },
        };

        let resolved = {
            let native = instance.router_obj.native.borrow();
            let Some(data) = native.as_ref().and_then(|n| n.downcast_ref::<RouterData>()) else {
                return RouteDispatch::Handled(HttpResponse::plain(500, "Internal Server Error"));
            };
            match target {
                Target::Route { route, params } => {
                    Some((data.handlers[route as usize].clone(), Some(route), params))
                }
                Target::Fallback => data
                    .fallback
                    .as_ref()
                    .map(|fb| (fb.clone(), None, ParamRanges::new())),
            }
        };
        let Some((handler, route_opt, params)) = resolved else {
            pool.idle.borrow_mut().push(instance);
            return RouteDispatch::Handled(HttpResponse::plain(404, "Not Found"));
        };

        let data = RequestData::new(request, Rc::clone(&pool.router), route_opt, params);
        let outcome = run_handler(&mut instance.vm, &handler, data).await;
        let (response, keep_instance) = match outcome {
            Ok(resp) => (resp, true),
            Err(error) => match instance.vm.report_unhandled(error) {
                Ok(()) => {
                    if !instance.vm.output.is_empty() {
                        eprint!("{}", String::from_utf8_lossy(&instance.vm.output));
                    }
                    (HttpResponse::plain(500, "Internal Server Error"), true)
                }
                // `exit()` in a handler discards this VM instance cleanly; the next
                // request forks a fresh instance from the route snapshot.
                Err(_) => {
                    if !instance.vm.output.is_empty() {
                        eprint!("{}", String::from_utf8_lossy(&instance.vm.output));
                    }
                    (HttpResponse::plain(500, "Internal Server Error"), false)
                }
            },
        };
        if keep_instance {
            instance.vm.output.clear();
            pool.idle.borrow_mut().push(instance);
        }
        RouteDispatch::Handled(response)
    }
}

async fn fork_instance(
    snapshot: &Snapshot,
    worker_index: usize,
    code: &Rc<crate::worker_code_cache::WorkerCodeCache>,
) -> Result<PooledVm, String> {
    let mut vm = snapshot.fork()?;
    vm.set_local_loader(Rc::new(crate::worker_code_cache::RequestCodeCache::new(
        code.clone(),
    )));
    let router_obj = vm
        .state_mut()
        .get::<ActiveRouterSlot>()
        .and_then(|slot| slot.0.clone())
        .ok_or_else(|| "route snapshot is missing its Phera\\Http\\Router".to_owned())?;
    let mut instance = PooledVm { vm, router_obj };
    run_on_start(&mut instance, worker_index).await?;
    Ok(instance)
}

async fn run_on_start(instance: &mut PooledVm, worker_index: usize) -> Result<(), String> {
    let callbacks: Vec<Value> = {
        let native = instance.router_obj.native.borrow();
        match native.as_ref().and_then(|n| n.downcast_ref::<RouterData>()) {
            Some(data) => data.on_start.clone(),
            None => Vec::new(),
        }
    };
    for callback in callbacks {
        if let Err(error) = instance
            .vm
            .call_callable(&callback, vec![Value::Int(worker_index as i64)])
            .await
        {
            let _ = instance.vm.report_unhandled(error);
            let output = String::from_utf8_lossy(&instance.vm.output)
                .trim()
                .to_owned();
            return Err(if output.is_empty() {
                "onStart callback failed".to_owned()
            } else {
                output
            });
        }
    }
    instance.vm.output.clear();
    Ok(())
}

async fn run_handler(
    vm: &mut Vm,
    handler: &Value,
    data: RequestData,
) -> Result<HttpResponse, phera_extension_api::NativeError> {
    let request = request::new_object(vm, data)?;
    let result = vm.call_callable(handler, vec![request]).await?;
    response::from_handler_result(vm, result).await
}

/// Checks whether a compiled unit mentions `Phera\Http\Router` or
/// `Phera\Http\serve` before executing it during `phera serve` boot.
fn unit_references_http_router(unit: &phera_bytecode::CompiledUnit) -> bool {
    unit.names
        .iter()
        .any(|name| name.eq_ignore_ascii_case("phera\\http\\router"))
        || unit.symbols.iter().any(|sym| {
            sym.name.eq_ignore_ascii_case("phera\\http\\router")
                || sym.name.eq_ignore_ascii_case("phera\\http\\serve")
        })
}
