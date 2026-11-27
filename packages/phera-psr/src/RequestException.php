<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Client\RequestExceptionInterface;
use Psr\Http\Message\RequestInterface;

/** The request cannot be sent as it is: a bad URI, method or header (PSR-18). */
final class RequestException extends \InvalidArgumentException implements RequestExceptionInterface
{
    public function __construct(private readonly RequestInterface $request, string $message, ?\Throwable $previous = null)
    {
        parent::__construct($message, 0, $previous);
    }

    public function getRequest(): RequestInterface
    {
        return $this->request;
    }
}
