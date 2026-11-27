<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * PSR-15 middleware in front of a handler: the first middleware runs
 * first, and each one's `$handler` is the rest of the pipeline.
 */
final class Pipeline implements RequestHandlerInterface
{
    /** @param list<MiddlewareInterface> $middleware */
    public function __construct(
        private readonly RequestHandlerInterface $handler,
        private readonly array $middleware = [],
        private readonly int $position = 0,
    ) {}

    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $middleware = $this->middleware[$this->position] ?? null;
        if ($middleware === null) {
            return $this->handler->handle($request);
        }
        return $middleware->process($request, new self($this->handler, $this->middleware, $this->position + 1));
    }
}
