# An application may declare warm-up requests

- Date: 2026-11-23
- Area: Server and requests, speed
- Decided by: Design

## Decision

An application can name paths that its worker handles before
`Phera\snapshot()`. It is off unless declared, in one of two places:

```toml
# phera.toml
[server]
warmup = ["/api/posts", "/"]
```

```sh
phera serve --warmup /api/posts --warmup / public
```

The option wins over the file. Both reach the framework adapters as the
environment variable `PHERA_WARMUP` (paths separated by white space), the way
`APP_RUNTIME` reaches symfony/runtime, so a platform that configures through
the environment can set that variable instead.
(`crates/cli/src/serve/config.rs`, `crates/runtime/src/adapters/`)

Each path is an anonymous `GET` (no cookie, no credentials, host of `APP_URL`
or `localhost`) handled through the framework's kernel and terminated like a
real request; its response is discarded. Then the framework's own reset for
long-running workers runs:

- **Symfony**: nothing is added. A booted kernel that has handled a request
  resets its services (`services_resetter`, every `kernel.reset` service)
  when the next request arrives, and it does so in every forked request.
- **Laravel**: after each warm-up request, what Octane does between two
  requests, written against the framework's own services because Octane need
  not be installed:

| Reset | Where Octane 2.x does it |
|---|---|
| the configuration's values put back | `CreateConfigurationSandbox` |
| locale and fallback locale (translator, Carbon) | `FlushLocaleState` |
| queued cookies | `FlushQueuedCookies` |
| every session store emptied and given a new id | `FlushSessionState` |
| `auth.driver` and the guards forgotten | `FlushAuthenticationState`, `Worker::handle` |
| per connection: open transactions rolled back, record modification state, query log, query duration | `FlushDatabaseRecordModificationState`, `FlushDatabaseQueryLog`, `RefreshQueryDurationHandling` (the roll-back is Phera's: a snapshot cannot hold a transaction) |
| the array cache store | `FlushArrayCache` |
| shared log context, resettable Monolog loggers | `FlushLogContext`, `FlushMonologState` |
| `Once`, `Str`'s caches, the translator's parsed keys, Vite | `FlushOnce`, `FlushStrCache`, `FlushTranslatorCache`, `FlushVite` |
| scoped instances | `FlushTemporaryContainerInstances` |
| the route's controller | `ApplicationGateway::terminate` |
| the stand-in request bound again; the router's current request and route, the compiled view engines | `Worker::handle` |
| the objects the facades remember | `CurrentApplication::set` |
| the application's terminating callbacks back to what they were | Octane throws its per-request copy of the application away |

A warm-up request that throws, or is answered with anything but a 2xx
status, is logged once per worker boot and the boot goes on.

The contract, as the documentation states it: a warm-up request must be an
anonymous `GET` that changes nothing. It runs once per worker boot, and a
worker boots again when it has learned classes and when a file changed.
Whatever it leaves in objects that outlive a request, and that the reset
above does not know, is what every request starts from.

## Why

LAYER2_NUMBERS

The warm-up without a request (the decision of the same date) can only build
what is safe for every application. Most of a route's first-use work is not
that: it is the application's own services, listeners, compiled routes and
loaded classes. Only the application can say that a request to one of its
routes is safe to run without a visitor.

## What is different from Octane, and what that means

Octane handles every request in a copy of the application container and
throws the copy away; services the request resolved die with it. A warm-up
request here runs in the application itself and what it resolved stays:
that is where the saving comes from. So a singleton that a warm-up request
built with something of that request inside (its URL, its locale, "the
current tenant") keeps it in every request that follows. An application
that is safe under Octane with that singleton in `octane.warm` is safe here.

For Symfony there is no such difference: a service that outlives a request
is the long-running kernel's ordinary case, and `kernel.reset` is its
contract. One consequence to know: a listener or service that the warm-up
request made Symfony build was built for that request. A constructor that
reads the current request keeps what it read.

## An authenticated route

Harmless, and reported. The warm-up request carries no cookie and no
credentials, so a route behind authentication answers 401, 403 or a
redirect; that is logged and nothing is kept from it but what any failed
request leaves. A route that signs a visitor in by itself (a development
shortcut) leaves no user either: the guards are forgotten and the session
is emptied after every warm-up request.

## Turned down

- **A new configuration file or a framework-specific setting** (Laravel
  `config/phera.php`, a symfony/runtime option): two mechanisms for one
  thing, and neither is where `phera serve` is configured today.
- **Replaying a recorded real request**: it would carry a visitor's cookies
  and headers into the state every request starts from.
- **Other methods than `GET`**: a warm-up request must change nothing.
- **Running Laravel's reset once, after the last path**: the user a
  warm-up route signed in would then be signed in for the warm-up requests
  after it.
- **Symfony: one `boot()` before the snapshot instead of the reset in every
  request.** It saves SF_RESET_COST per request, and every request of a
  worker would then share what a reset is meant to renew per request (a
  Monolog `UidProcessor`'s id, for one).
- **Stopping the boot when a warm-up request fails**: a route that is
  broken, moved or now behind a login would take the whole worker down.

## Structure: was, is

| Structure | Was | Is |
|---|---|---|
| State at `Phera\snapshot()` | the booted framework plus the warm-up without a request | the same plus everything the declared requests built and the reset left: resolved services, compiled routes, listeners, loaded classes, connection objects without sockets |
| Where the paths live | nowhere | `[server] warmup` in `phera.toml` or `--warmup`, carried to the adapters in `PHERA_WARMUP` |
