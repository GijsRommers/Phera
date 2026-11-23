Partly replaced by `2026-11-20-snapshots-are-the-only-mode.md`: there is no `--no-snapshot`.

# Adapters are PHP shipped in the binary, compiled under the application's names

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Laravel (a project with `artisan` and `bootstrap/app.php` above the document
root): `phera serve` runs an entry script it ships in place of
`public/index.php`, compiled under that file's name. It loads Composer,
creates the application, bootstraps the HTTP kernel (providers registered
and booted), calls `Phera\snapshot()`, then checks maintenance mode and
calls `handleRequest()` on the line where Laravel's own `public/index.php`
calls it. Symfony with `symfony/runtime` (a `vendor/autoload_runtime.php`):
`phera serve` sets `APP_RUNTIME=Phera\Symfony\Runtime` unless something set
it; that class is PHP shipped in the binary, compiled by the VM the first
time a script uses a class in the `Phera\` namespace that no file declared
(before any autoloader runs). Its runner boots the kernel, snapshots, and
then does exactly what Symfony's `HttpKernelRunner` does. `phera serve
--no-snapshot` turns adapters and boots off.
(`crates/runtime/src/adapters/`, `crates/vm/src/bundled_classes.rs`)

## Why

No file is written into the project and no Composer package is needed.
Compiling the Laravel entry under `public/index.php`'s name keeps `__DIR__`,
`get_included_files()` and the stack traces that a debug error page shows
the same as the classic way (the trace's last frame is `public/index.php`,
line 20), which byte-identical responses need. The bundled class is
invisible to `spl_autoload_functions()`.

## Turned down

Generating files into the project (writes into a directory Phera does not
own); an autoloader registered at request start (visible to scripts);
detecting Laravel by reading `public/index.php`. A Symfony front controller
whose closure takes a `Request` argument gets the boot's empty request:
symfony/runtime resolves the closure's arguments before the runner runs, so
such an application should take `array $context`, as Symfony's recipes do.
