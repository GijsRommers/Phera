<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Message\UriInterface;

/**
 * Method, URI and request target of a PSR-7 request.
 *
 * @internal
 */
trait RequestTrait
{
    use MessageTrait;

    private string $method;

    private UriInterface $uri;

    private ?string $requestTarget = null;

    /** @param array<string, string|list<string>> $headers */
    private function initialize(string $method, UriInterface|string $uri, array $headers, mixed $body, string $protocol): void
    {
        if (preg_match('/^[!#$%&\'*+\-.^_`|~0-9A-Za-z]+$/D', $method) !== 1) {
            throw new \InvalidArgumentException("\"$method\" is not a valid HTTP method");
        }
        $this->method = $method;
        $this->uri = $uri instanceof UriInterface ? $uri : new Uri($uri);
        $this->protocol = $protocol;
        $this->setHeaders($headers);
        if (!$this->hasHeader('Host')) {
            $this->updateHostFromUri();
        }
        if ($body !== null && $body !== '') {
            $this->stream = $body instanceof \Psr\Http\Message\StreamInterface ? $body : Stream::create((string) $body);
        }
    }

    public function getRequestTarget(): string
    {
        if ($this->requestTarget !== null) {
            return $this->requestTarget;
        }
        $target = $this->uri->getPath();
        if ($target === '') {
            $target = '/';
        }
        $query = $this->uri->getQuery();
        return $query === '' ? $target : "$target?$query";
    }

    public function withRequestTarget(string $requestTarget): static
    {
        if (preg_match('/\s/', $requestTarget) === 1) {
            throw new \InvalidArgumentException('A request target must not contain whitespace');
        }
        $copy = clone $this;
        $copy->requestTarget = $requestTarget;
        return $copy;
    }

    public function getMethod(): string
    {
        return $this->method;
    }

    public function withMethod(string $method): static
    {
        if (preg_match('/^[!#$%&\'*+\-.^_`|~0-9A-Za-z]+$/D', $method) !== 1) {
            throw new \InvalidArgumentException("\"$method\" is not a valid HTTP method");
        }
        $copy = clone $this;
        $copy->method = $method;
        return $copy;
    }

    public function getUri(): UriInterface
    {
        return $this->uri;
    }

    public function withUri(UriInterface $uri, bool $preserveHost = false): static
    {
        if ($uri === $this->uri) {
            return $this;
        }
        $copy = clone $this;
        $copy->uri = $uri;
        if (!$preserveHost || !$copy->hasHeader('Host')) {
            $copy->updateHostFromUri();
        }
        return $copy;
    }

    /** `Host` comes first, as RFC 7230 asks. */
    private function updateHostFromUri(): void
    {
        $host = $this->uri->getHost();
        if ($host === '') {
            return;
        }
        $port = $this->uri->getPort();
        if ($port !== null) {
            $host .= ":$port";
        }
        $this->removeHeader('Host');
        $this->headerNames = ['host' => 'Host'] + $this->headerNames;
        $this->headers = ['Host' => [$host]] + $this->headers;
    }
}
