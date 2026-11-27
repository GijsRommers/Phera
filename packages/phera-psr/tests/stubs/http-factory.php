<?php

// The interfaces of psr/http-factory 1.1, for running the tests offline.

namespace Psr\Http\Message;

interface RequestFactoryInterface
{
    /** @param UriInterface|string $uri */
    public function createRequest(string $method, $uri): RequestInterface;
}

interface ResponseFactoryInterface
{
    public function createResponse(int $code = 200, string $reasonPhrase = ''): ResponseInterface;
}

interface ServerRequestFactoryInterface
{
    /** @param UriInterface|string $uri */
    public function createServerRequest(string $method, $uri, array $serverParams = []): ServerRequestInterface;
}

interface StreamFactoryInterface
{
    public function createStream(string $content = ''): StreamInterface;
    public function createStreamFromFile(string $filename, string $mode = 'r'): StreamInterface;
    /** @param resource $resource */
    public function createStreamFromResource($resource): StreamInterface;
}

interface UriFactoryInterface
{
    public function createUri(string $uri = ''): UriInterface;
}
