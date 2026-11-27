<?php

declare(strict_types=1);

// A PSR-15 application served by Phera\Http\Router: `phera serve tests/app.php`.

require __DIR__ . '/bootstrap.php';

use Phera\Http\Router;
use Phera\Psr\Psr15Bridge;
use Phera\Psr\Response;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;

final class Describe implements RequestHandlerInterface
{
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        if ($request->getUri()->getPath() === '/redirect') {
            return new Response(302, ['Location' => '/psr/elsewhere']);
        }
        $body = json_encode([
            'method' => $request->getMethod(),
            'path' => $request->getUri()->getPath(),
            'target' => $request->getRequestTarget(),
            'query' => $request->getQueryParams(),
            'attributes' => $request->getAttributes(),
            'parsed' => $request->getParsedBody(),
            'cookies' => $request->getCookieParams(),
            'tag' => $request->getHeaderLine('X-Tag'),
            'body' => (string) $request->getBody(),
        ], JSON_THROW_ON_ERROR);
        return new Response(200, ['Content-Type' => 'application/json', 'X-Multi' => ['1', '2']], $body);
    }
}

final class Timing implements MiddlewareInterface
{
    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        return $handler->handle($request->withAttribute('timing', 'on'))->withHeader('X-Middleware', 'timing');
    }
}

final class Guard implements MiddlewareInterface
{
    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        if ($request->getHeaderLine('X-Block') === 'yes') {
            return new Response(403, ['Content-Type' => 'text/plain'], 'blocked');
        }
        return $handler->handle($request);
    }
}

$router = new Router();
$router->any('/psr/*rest', Psr15Bridge::handler(new Describe(), new Timing(), new Guard()));
$router->any('/redirect', Psr15Bridge::handler(new Describe()));
return $router;
