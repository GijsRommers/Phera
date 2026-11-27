<?php

declare(strict_types=1);

namespace Phera\Psr;

use Phera\Http\Request as PheraRequest;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Message\StreamInterface;
use Psr\Http\Message\UriInterface;

/**
 * A PSR-7 server request, usually made from the `Phera\Http\Request` a
 * router hands its handler:
 *
 * ```php
 * $psr = ServerRequest::fromPhera($request);
 * $psr->getAttribute('id');      // route parameters are attributes
 * ```
 */
final class ServerRequest implements ServerRequestInterface
{
    use RequestTrait;

    private array $cookieParams = [];

    private ?array $queryParams = null;

    private array $uploadedFiles = [];

    private mixed $parsedBody = null;

    private array $attributes = [];

    /**
     * @param array<string, string|list<string>> $headers
     * @param array<string, mixed> $serverParams
     */
    public function __construct(
        string $method,
        UriInterface|string $uri,
        array $headers = [],
        StreamInterface|string|null $body = null,
        string $version = '1.1',
        private readonly array $serverParams = [],
    ) {
        $this->initialize($method, $uri, $headers, $body, $version);
    }

    /**
     * The PSR-7 view of a router's request: its headers and body, the query
     * and cookies parsed, form bodies parsed, route parameters as attributes.
     */
    public static function fromPhera(PheraRequest $request): self
    {
        $headers = $request->headers();
        $host = $request->header('host') ?? 'localhost';
        $scheme = strtolower($request->header('x-forwarded-proto') ?? 'http') === 'https' ? 'https' : 'http';
        $target = $request->uri();
        $psr = new self(
            $request->method(),
            new Uri("$scheme://$host$target"),
            $headers,
            $request->body(),
            '1.1',
            [
                'REQUEST_METHOD' => $request->method(),
                'REQUEST_URI' => $target,
                'SERVER_PROTOCOL' => 'HTTP/1.1',
                'HTTP_HOST' => $host,
                'HTTPS' => $scheme === 'https' ? 'on' : '',
            ],
        );
        $psr->requestTarget = $target;
        $psr->queryParams = $request->query();
        $psr->attributes = $request->params();
        $cookie = $request->header('cookie');
        if ($cookie !== null) {
            $psr->cookieParams = self::parseCookies($cookie);
        }
        $type = strtolower($request->header('content-type') ?? '');
        if (str_starts_with($type, 'application/x-www-form-urlencoded')) {
            parse_str($request->body(), $form);
            $psr->parsedBody = $form;
        }
        return $psr;
    }

    public function getServerParams(): array
    {
        return $this->serverParams;
    }

    public function getCookieParams(): array
    {
        return $this->cookieParams;
    }

    public function withCookieParams(array $cookies): static
    {
        $copy = clone $this;
        $copy->cookieParams = $cookies;
        return $copy;
    }

    public function getQueryParams(): array
    {
        if ($this->queryParams === null) {
            parse_str($this->uri->getQuery(), $query);
            $this->queryParams = $query;
        }
        return $this->queryParams;
    }

    public function withQueryParams(array $query): static
    {
        $copy = clone $this;
        $copy->queryParams = $query;
        return $copy;
    }

    public function getUploadedFiles(): array
    {
        return $this->uploadedFiles;
    }

    public function withUploadedFiles(array $uploadedFiles): static
    {
        $copy = clone $this;
        $copy->uploadedFiles = $uploadedFiles;
        return $copy;
    }

    public function getParsedBody()
    {
        return $this->parsedBody;
    }

    public function withParsedBody($data): static
    {
        if (!is_array($data) && !is_object($data) && $data !== null) {
            throw new \InvalidArgumentException('The parsed body must be an array, an object or null');
        }
        $copy = clone $this;
        $copy->parsedBody = $data;
        return $copy;
    }

    public function getAttributes(): array
    {
        return $this->attributes;
    }

    public function getAttribute(string $name, $default = null)
    {
        return array_key_exists($name, $this->attributes) ? $this->attributes[$name] : $default;
    }

    public function withAttribute(string $name, $value): static
    {
        $copy = clone $this;
        $copy->attributes[$name] = $value;
        return $copy;
    }

    public function withoutAttribute(string $name): static
    {
        if (!array_key_exists($name, $this->attributes)) {
            return $this;
        }
        $copy = clone $this;
        unset($copy->attributes[$name]);
        return $copy;
    }

    /** @return array<string, string> */
    private static function parseCookies(string $header): array
    {
        $cookies = [];
        foreach (explode(';', $header) as $pair) {
            [$name, $value] = array_pad(explode('=', trim($pair), 2), 2, '');
            if ($name !== '' && !array_key_exists($name, $cookies)) {
                $cookies[$name] = urldecode($value);
            }
        }
        return $cookies;
    }
}
