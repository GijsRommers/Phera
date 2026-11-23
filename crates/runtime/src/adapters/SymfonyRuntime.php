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
     * Builds, before the snapshot, what nearly every request would build
     * first. Whatever exists at Phera\snapshot() is not built again by any
     * request.
     *
     * No request exists while this runs, so it only builds what Symfony
     * itself builds once and keeps for many requests when a kernel is
     * long-running, and it runs no code of the application: the HTTP
     * kernel with its event dispatcher (listeners are not created, they
     * stay lazy), the router's matcher loaded from the cache the build
     * warmed, and Doctrine's metadata of the mapped entities. No connection
     * is opened. `phera serve --no-service-warmup` turns it off.
     */
    private function warmUp(KernelInterface $kernel): void
    {
        if ('0' === ($_SERVER['PHERA_SERVICE_WARMUP'] ?? '')) {
            return;
        }

        $container = $kernel->getContainer();
        $steps = [
            static fn () => $container->get('http_kernel'),
            static fn () => $container->get('router')->getMatcher(),
            static function () use ($container) {
                $doctrine = $container->get('doctrine');
                foreach ($doctrine->getManagerNames() as $name => $service) {
                    $doctrine->getManager($name)->getMetadataFactory()->getAllMetadata();
                }
            },
        ];
        foreach ($steps as $step) {
            try {
                $step();
            } catch (\Throwable) {
                // A service the application does not have, or one that
                // cannot be built yet: the request that needs it reports
                // it, as it always did.
            }
        }
    }
}
