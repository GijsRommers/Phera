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
}
