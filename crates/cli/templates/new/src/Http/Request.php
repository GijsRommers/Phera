<?php

declare(strict_types=1);

namespace App\Http;

final class Request
{
    /**
     * @param array<string, mixed> $query the values after the "?" in the address
     */
    public function __construct(
        public readonly string $method,
        public readonly string $path,
        private readonly array $query = [],
    ) {
    }

    public static function fromGlobals(): self
    {
        // "/hello?name=x" is the path "/hello" and a query; only the path chooses the route.
        $path = explode('?', $_SERVER['REQUEST_URI'] ?? '/', 2)[0];

        return new self($_SERVER['REQUEST_METHOD'] ?? 'GET', $path, $_GET);
    }

    public function query(string $name, mixed $default = null): mixed
    {
        return $this->query[$name] ?? $default;
    }
}
