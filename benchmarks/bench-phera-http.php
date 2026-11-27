<?php

/**
 * Benchmark routes for unified `phera serve` + `Phera\Http\Router`:
 *   phera serve --listen 127.0.0.1:8791 --workers 2 benchmarks/bench-phera-http.php
 */

use Phera\Http\Request;
use Phera\Http\Response;
use Phera\Http\Router;

$router = new Router();

$router->get('/plaintext', static fn(): Response => Response::text('Hello, World!'));

$router->get('/json', static fn(): Response => Response::json(['message' => 'Hello, World!']));

$router->get('/users/{id}', static function (Request $req): Response {
    return Response::json(['id' => (int) $req->param('id')]);
});

return $router;
