<?php

declare(strict_types=1);

namespace Phera\Psr;

use Phera\Http\Request as PheraRequest;
use Phera\Http\Response as PheraResponse;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

/**
 * Serves a PSR-15 application from `Phera\Http\Router`:
 *
 * ```php
 * $router->any('/*path', Psr15Bridge::handler($app, new SessionMiddleware(), new AuthMiddleware()));
 * return $router;   // phera serve app.php
 * ```
 *
 * Each request becomes a {@see ServerRequest} (route parameters as
 * attributes), runs through the middleware in order and the handler, and
 * the PSR-7 response goes out as a `Phera\Http\Response`.
 */
final class Psr15Bridge
{
    /** @return \Closure(PheraRequest): PheraResponse */
    public static function handler(RequestHandlerInterface $handler, MiddlewareInterface ...$middleware): \Closure
    {
        $pipeline = $middleware === [] ? $handler : new Pipeline($handler, array_values($middleware));
        return static fn (PheraRequest $request): PheraResponse
            => Response::toPhera($pipeline->handle(ServerRequest::fromPhera($request)));
    }
}
