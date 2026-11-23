# The adapters warm up before the snapshot, without a request

- Date: 2026-11-23
- Area: Server and requests, speed
- Decided by: Design

## Decision

Between booting the framework and `Phera\snapshot()`, both adapters build
what nearly every request would otherwise build first. It is on for every
Laravel and Symfony application; `phera serve --no-service-warmup` (or
`PHERA_SERVICE_WARMUP=0` in the environment) turns it off.
(`crates/runtime/src/adapters/LaravelWarmUp.php`, `SymfonyRuntime.php`)

**Laravel**, in this order:

1. Every Eloquent model of the application is booted and one query is built
   for it, `(new $class)->newQueryWithoutScopes()->toSql()`. Nothing is sent
   to the database and no connection is opened. The models are found the way
   the framework's own `model:prune` finds them: the PHP files under
   `app/Models`, named by the application's namespace and their path, kept
   when they are concrete subclasses of `Model`.
2. The services of the application's `octane.warm` list are resolved when it
   has one, of Octane's default list otherwise
   (`Laravel\Octane\Octane::defaultServicesToWarm()`), with the request
   Laravel's console binds (`SetRequestForConsole`: a `GET` of `APP_URL`)
   standing in for the request. `session.store` is left out.
3. `auth->forgetGuards()`, `forgetScopedInstances()`, and a session store
   that step 1 or 2 caused is forgotten, in the session manager too.

**Symfony**: the event dispatcher is built (`event_dispatcher`), which
registers its listeners without creating any of them. Nothing else, and not
in a debug kernel.

A step that throws is skipped, silently: the application then behaves as it
does without the warm-up, and the request that needs the thing reports the
problem as it always did.

What a request can observe of it: nothing, on the test applications (the
complete responses are the same with it, without it, and under PHP). The
stated differences, which a program could construct:

- A model's `boot()`, `booted()` and trait boots run once per worker boot
  instead of once per request that uses the model. Code there that asks for
  the request or the signed-in user fails, because neither exists yet, and
  the model is put back exactly as it was (Eloquent's static state, and the
  events that gained their first listener) and boots in the request. Code
  there that reads `$_SERVER`, a cache or the clock without going through
  the request gets the boot's answer for every request. That is the contract
  a service provider's `boot()` already has under `phera serve`.
- The console stand-in request stays bound in the container after the
  warm-up, as it does in an Octane worker. The HTTP kernel binds the real
  request before any code of the application runs.

## Why

A fork is a private copy of the worker's state at the snapshot, so whatever
exists there is not built again by any request. A review measured the idea on
scratch copies of the test applications (2026-11-22); these are the product's
adapters, same binary with the warm-up on and off, one worker, instructions
per settled request (lowest of the pairs):

| Route | Off | On | Change | PHP-FPM |
|---|---:|---:|---:|---:|
| Laravel `/api/posts?page=2` | LPOFF | LPON | LPCH | 16.7M |
| Laravel `/api/hello` | LHOFF | LHON | LHCH | 7.4M |
| Symfony `/api/ping` | SPOFF | SPON | SPCH | |
| Symfony `/api/db-yardstick` | SDOFF | SDON | SDCH | 57.9M |

Where it comes from, each step alone (Laravel, main `1fcc6383`):

| Step | `/api/hello` | `/api/posts?page=2` |
|---|---:|---:|
| nothing | 25.20M | 106.37M |
| services | 22.02M | 106.05M |
| a model class of the adapter's own, one query | 25.70M | 105.55M |
| the application's models booted | 25.49M | 72.38M |
| ... and one query built for each, without scopes | 26.98M | 55.16M |
| ... the same with `Model::query()` | 26.96M | 55.15M |
| services, models, query | 23.88M | 54.98M |
| ... and the default connection opened | 23.90M | 54.69M |
| ... and all deferred providers loaded | 24.58M | 55.71M |

Symfony, each candidate alone (main `1fcc6383`; "fork" is a request that
ends right after `Phera\snapshot()`):

| Step | `/api/ping` | `/api/db-yardstick` | fork |
|---|---:|---:|---:|
| nothing | 14.16M | 442.56M | 1.63M |
| the event dispatcher (kept) | 13.35M | 442.59M | 1.86M |
| `http_kernel` | 12.88M | 441.44M | |
| the router's matcher | 13.26M | 441.81M | |
| `http_kernel` and matcher | 12.10M | 440.54M | 2.10M |
| the listeners of the kernel events | 10.05M | 438.49M | |
| the entity manager | 14.08M | 441.63M | |
| Doctrine's metadata of all entities | 15.47M | 428.76M | 3.19M |
| kernel, matcher, metadata | 13.48M | 427.65M | |
| ... and `services_resetter` run in every request | 15.04M | 428.13M | |

The cost: a fork copies what the snapshot holds, so every request pays for
what the warm-up built, whether it uses it or not. FORK_COST

The boot of a worker: BOOT_COST

## Turned down

- **`Model::query()` for the query.** It applies the model's global scopes,
  which are application code written to run per query, with a user. Building
  the query without them is worth the same (55.16M against 55.15M).
- **Booting the models with the stand-in request bound.** Boot code that
  reads the request would then quietly get `APP_URL` and no user, and every
  request would inherit the result. Without a request it fails, and the
  failure is what tells the adapter to leave that model to the request.
- **Opening the database connection.** 0.3M on a 55M request, for a boot
  that would depend on the database being up.
- **Loading all deferred providers**, as Octane does: slower on both routes.
- **`session.store` in the service list.** A store draws its random id when
  it is built. Octane replaces it before every request
  (`FlushSessionState`); nothing of Phera runs before a request, so every
  request of a worker would start from the same id.
- **Symfony: the listeners of the kernel events** (28% on `/api/ping`).
  Listeners are services of the application. Symfony creates one when its
  event is first dispatched, which is inside a request, so a constructor may
  read the current request; created before the snapshot it would read
  "none" and keep that for every request. A test application has such a
  listener.
- **Symfony: `http_kernel` and the router's matcher** (14.5% on
  `/api/ping` together). Both take the logger, and building the logger
  builds what the application configured for it. A Monolog `UidProcessor`
  draws its id in its constructor: every request of a worker would log the
  same one. Running `services_resetter` in every request renews what is
  resettable, costs 0.6M per request, and does nothing for a processor of
  the application's own that is not.
- **Symfony: Doctrine's metadata of all entities.** It takes 14M off the
  Doctrine route and puts 1.4M on every route that does not use Doctrine,
  for two entities: class metadata is a graph of objects, and a fork copies
  every object the snapshot holds. The cost grows with the number of
  entities, on every route. Declared warm-up requests load only what their
  routes use.
- **Removing the stand-in request after the warm-up.** `Container::instance()`
  calls the rebinding callbacks only when the name was bound, so the URL
  generator would keep `APP_URL` as its host in every request.
- **A list of models in configuration.** The directory scan costs SCAN_COST.

## Structure: was, is

| Structure | Was | Is |
|---|---|---|
| State at `Phera\snapshot()`, Laravel | the application with its providers booted | the same, plus the booted models (Eloquent's static tables), one connection object without a socket, and the resolved services of Octane's list |
| State at `Phera\snapshot()`, Symfony | the booted kernel and its container | the same, plus the HTTP kernel, the router's matcher and the loaded class metadata |
| Laravel's entry | 12 lines compiled as `public/index.php` | the same lines plus one call into `Phera\Laravel\WarmUp`, a class shipped in the binary like `Phera\Symfony\Runtime` |

No cache is added: nothing is looked up through the warm-up, the objects
simply exist earlier.
