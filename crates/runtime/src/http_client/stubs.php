<?php

/**
 * Native signature declarations for Phera\Http\Client, checked by
 * crates/runtime/src/native_parameter_tests.rs. Also usable as IDE stubs.
 */

namespace Phera\Http;

/**
 * HTTP/1.1 and HTTP/2 requests over pooled keep-alive connections.
 *
 * Options: `base_uri` (string), `timeout` (seconds, default 30.0, 0 for
 * none), `connect_timeout` (seconds, default 10.0), `headers` (array),
 * `max_redirects` (int, default 5, 0 to return redirects as they are),
 * `verify` (bool, default true), `http2` (bool, default true: offered to
 * TLS servers), `http2_prior_knowledge` (bool, default false: h2c).
 *
 * Request options: `headers`, `query` (array or string), `body` (string),
 * `json` (any value, sent as JSON), `timeout`, `max_redirects`.
 */
final class Client
{
    public function __construct(array $options = []) {}

    /** @throws ClientException when no response arrives (connect, TLS, timeout, redirects) */
    public function request(string $method, string $uri, array $options = []): Response {}

    public function get(string $uri, array $options = []): Response {}

    public function post(string $uri, array $options = []): Response {}

    public function put(string $uri, array $options = []): Response {}

    public function patch(string $uri, array $options = []): Response {}

    public function delete(string $uri, array $options = []): Response {}

    public function head(string $uri, array $options = []): Response {}

    /**
     * Sends `[$key => ['method' => 'GET', 'uri' => '/a', ...options]]` at once;
     * returns `[$key => Response]`.
     *
     * @return array<array-key, Response>
     */
    public function pool(array $requests): array {}
}

/** A request got no response: connection, TLS, timeout or too many redirects. */
class ClientException extends \RuntimeException {}
