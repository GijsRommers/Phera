<?php

/**
 * Native signature declarations for Phera\Http, checked by
 * crates/runtime/src/native_parameter_tests.rs. Also usable as IDE stubs.
 */

namespace Phera\Http;

function serve(Router $router, string $addr = '127.0.0.1:8080', int $workers = 0): void {}

final class Router
{
    public function __construct() {}

    public function get(string $path, callable $handler): static {}

    public function post(string $path, callable $handler): static {}

    public function put(string $path, callable $handler): static {}

    public function patch(string $path, callable $handler): static {}

    public function delete(string $path, callable $handler): static {}

    public function head(string $path, callable $handler): static {}

    public function options(string $path, callable $handler): static {}

    public function any(string $path, callable $handler): static {}

    public function route(string $method, string $path, callable $handler): static {}

    public function fallback(callable $handler): static {}

    /** `$callback(int $worker)` runs once per persistent worker VM instance before it serves requests. */
    public function onStart(callable $callback): static {}
}

final class Request
{
    private function __construct() {}

    public function method(): string {}

    public function path(): string {}

    public function uri(): string {}

    public function param(string $name, ?string $default = null): ?string {}

    public function params(): array {}

    public function query(?string $name = null, mixed $default = null): mixed {}

    public function header(string $name, ?string $default = null): ?string {}

    public function headers(): array {}

    public function body(): string {}

    public function json(bool $associative = true): mixed {}
}

final class Response
{
    private function __construct() {}

    public static function text(string $body, int $status = 200, array $headers = []): static {}

    public static function html(string $body, int $status = 200, array $headers = []): static {}

    public static function json(mixed $data, int $status = 200, array $headers = []): static {}

    public static function bytes(string $body, string $contentType = 'application/octet-stream', int $status = 200, array $headers = []): static {}

    public static function empty(int $status = 204, array $headers = []): static {}

    /** A response with exactly `$headers`: no default `Content-Type`. */
    public static function make(string $body = '', int $status = 200, array $headers = []): static {}

    public function withStatus(int $status): static {}

    public function withHeader(string $name, string $value): static {}

    public function status(): int {}

    public function header(string $name, ?string $default = null): ?string {}

    public function headers(): array {}

    public function body(): string {}

    /** `json_decode($body, $associative, 512, JSON_THROW_ON_ERROR)`; `null` for an empty body. */
    public function decodeJson(bool $associative = true): mixed {}
}
