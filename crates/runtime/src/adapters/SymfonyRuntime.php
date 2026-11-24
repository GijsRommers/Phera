<?php

/*
 * Phera's runtime for Symfony applications that use symfony/runtime.
 *
 * `phera serve` sets APP_RUNTIME to this class when the application uses
 * symfony/runtime and nothing else chose a runtime. It is Symfony's own
 * runtime, except for HTTP: the kernel boots and warms up once per worker,
 * before the first request, and every request continues after
 * Phera\snapshot() with a copy of that kernel. What the runner does after
 * the call is what Symfony's HttpKernelRunner does.
 *
 * An application can name requests to handle before the snapshot
 * (`phera serve --warmup /some/path`): see handleWarmUpRequests().
 */

namespace Phera\Symfony;

use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\HttpKernelInterface;
use Symfony\Component\HttpKernel\KernelInterface;
use Symfony\Component\HttpKernel\TerminableInterface;
use Symfony\Component\Runtime\RunnerInterface;
use Symfony\Component\Runtime\SymfonyRuntime;

class Runtime extends SymfonyRuntime
{
    public function getRunner(?object $application): RunnerInterface
    {
        if ($application instanceof HttpKernelInterface && !($_SERVER['FRANKENPHP_WORKER'] ?? false)) {
            return new SnapshotRunner($application, $this->options['debug'] ?? false);
        }

        return parent::getRunner($application);
    }
}

final class SnapshotRunner implements RunnerInterface
{
    public function __construct(
        private readonly HttpKernelInterface $kernel,
        private readonly bool $debug,
    ) {
    }

    public function run(): int
    {
        if ($this->kernel instanceof KernelInterface) {
            $this->kernel->boot();
            $this->warmUp($this->kernel);
        }

        $paths = preg_split('/\s+/', $_SERVER['PHERA_WARMUP'] ?? '', -1, \PREG_SPLIT_NO_EMPTY);
        if ([] !== $paths) {
            $this->handleWarmUpRequests($this->kernel, $paths);
        }

        if ($this->kernel instanceof KernelInterface && \function_exists('Phera\\WebSocket\\resolveUsing')) {
            $container = $this->kernel->getContainer();
            \Phera\WebSocket\resolveUsing(static function (string $class) use ($container): object {
                return $container->has($class) ? $container->get($class) : new $class();
            });
        }

        \Phera\snapshot();

        $request = Request::createFromGlobals();
        $response = $this->kernel->handle($request);

        $response->send(false);

        if (\function_exists('fastcgi_finish_request') && !$this->debug) {
            fastcgi_finish_request();
        } elseif (\function_exists('litespeed_finish_request') && !$this->debug) {
            litespeed_finish_request();
        } else {
            Response::closeOutputBuffers(0, true);
            flush();
        }

        if ($this->kernel instanceof TerminableInterface) {
            $this->kernel->terminate($request, $response);
        }

        return 0;
    }

    /**
     * Builds, before the snapshot, what every request would build first and
     * what is safe to build without one: the event dispatcher. Whatever
     * exists at Phera\snapshot() is not built again by any request.
     *
     * Building the dispatcher registers its listeners without creating
     * them: Symfony creates a listener when its event is first dispatched,
     * inside a request, and that stays so. Nothing else is built here,
     * because everything else creates services the application defines
     * (the HTTP kernel and the router take the logger, with its processors
     * and handlers), and a service created before the snapshot keeps for
     * every request whatever its constructor read or drew.
     * `phera serve --no-service-warmup` turns this off.
     */
    private function warmUp(KernelInterface $kernel): void
    {
        // A debug kernel wraps the dispatcher in one that records and logs.
        if ('0' === ($_SERVER['PHERA_SERVICE_WARMUP'] ?? '') || $kernel->isDebug()) {
            return;
        }

        try {
            $kernel->getContainer()->get('event_dispatcher');
        } catch (\Throwable) {
            // The request that needs it reports it, as it always did.
        }
    }

    /**
     * Handles the warm-up requests the application declared
     * (`phera serve --warmup PATH`, or `warmup` in phera.toml): each path is
     * an anonymous GET through the kernel, terminated like a real request.
     * Its response goes nowhere.
     *
     * Nothing has to be reset here. A booted kernel that has handled a
     * request resets its services when the next one arrives
     * (`services_resetter`: every service tagged `kernel.reset`, which
     * clears the entity manager, empties the token storage, forgets the PHP
     * session and renews a logger's request id). Every request is a copy
     * of this kernel, so every request starts with that reset, as each
     * request of a long-running Symfony worker does.
     *
     * Unlike warmUp(), this runs the application's controllers and builds
     * its listeners. What a warm-up request leaves in a service that is not
     * reset is what every request starts from; see the documentation of
     * `--warmup`.
     *
     * @param list<string> $paths
     */
    private function handleWarmUpRequests(HttpKernelInterface $kernel, array $paths): void
    {
        foreach ($paths as $path) {
            $request = Request::create('/'.ltrim($path, '/'));
            $buffers = ob_get_level();
            ob_start();
            try {
                $response = $kernel->handle($request);
                if ($kernel instanceof TerminableInterface) {
                    $kernel->terminate($request, $response);
                }
                $status = $response->getStatusCode();
                $problem = $status >= 200 && $status < 300 ? null : "was answered with status $status";
            } catch (\Throwable $error) {
                $problem = 'threw '.$error::class.': '.$error->getMessage();
            }
            while (ob_get_level() > $buffers) {
                ob_end_clean();
            }
            if (null !== $problem) {
                error_log("the warm-up request $path $problem; the worker starts without what it would have built");
            }
        }
    }
}
