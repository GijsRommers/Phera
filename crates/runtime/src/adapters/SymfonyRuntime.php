<?php

/*
 * Phera's runtime for Symfony applications that use symfony/runtime.
 *
 * `phera serve` sets APP_RUNTIME to this class when the application uses
 * symfony/runtime and nothing else chose a runtime. It is Symfony's own
 * runtime, except for HTTP: the kernel boots once per worker, before the
 * first request, and every request continues after Phera\snapshot() with
 * a copy of the booted kernel. What the runner does after the call is what
 * Symfony's HttpKernelRunner does.
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
}
