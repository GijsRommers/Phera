<?php

// The interfaces of psr/http-client 1.0, for running the tests offline.

namespace Psr\Http\Client;

use Psr\Http\Message\RequestInterface;
use Psr\Http\Message\ResponseInterface;

interface ClientInterface
{
    /** @throws ClientExceptionInterface */
    public function sendRequest(RequestInterface $request): ResponseInterface;
}

interface ClientExceptionInterface extends \Throwable {}

interface RequestExceptionInterface extends ClientExceptionInterface
{
    public function getRequest(): RequestInterface;
}

interface NetworkExceptionInterface extends ClientExceptionInterface
{
    public function getRequest(): RequestInterface;
}
