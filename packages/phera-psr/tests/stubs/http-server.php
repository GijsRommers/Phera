<?php

// The interfaces of psr/http-server-handler and psr/http-server-middleware
// 1.0, for running the tests offline.

namespace Psr\Http\Server;

use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;

interface RequestHandlerInterface
{
    public function handle(ServerRequestInterface $request): ResponseInterface;
}

interface MiddlewareInterface
{
    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface;
}
