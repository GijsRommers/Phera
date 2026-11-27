<?php

/**
 * Native signature declarations for Phera\Http, checked by
 * crates/runtime/src/native_parameter_tests.rs. Also usable as IDE stubs.
 */

namespace Phera\Http;

final class Server
{
    /** `$workers: 0` starts one worker per CPU. The constructor binds the socket. */
    public function __construct(string $addr = '127.0.0.1:8080', int $workers = 0) {}

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

    /** `$callback(int $worker)` runs in each worker before it accepts requests. */
    public function onWorkerStart(callable $callback): static {}

    public function address(): string {}

    public function workers(): int {}

    public function stop(): void {}

    public function listen(): void {}
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

    public function withStatus(int $status): static {}

    public function withHeader(string $name, string $value): static {}

    public function status(): int {}

    public function header(string $name): ?string {}

    public function headers(): array {}

    public function body(): string {}
}
