<?php

/**
 * Declarations of Phera\Grpc and Phera\Protobuf, for IDEs and static
 * analysis. crates/runtime/src/native_parameter_tests.rs checks them against
 * the native registration.
 */

namespace Phera\Protobuf {

    class Exception extends \RuntimeException {}

    final class Schema
    {
        public static function fromDescriptor(string $descriptorSet): Schema {}

        /** @param array<string, array<int, array>> $messages */
        public static function define(array $messages, array $services = []): Schema {}

        public function encode(string $messageType, array $data): string {}

        public function decode(string $messageType, string $bytes): array {}

        /** @return list<string> */
        public function messages(): array {}

        /** @return array<string, array{input: string, output: string, clientStreaming: bool, serverStreaming: bool}> */
        public function methods(): array {}
    }
}

namespace Phera\Grpc {

    final class Status
    {
        public const OK = 0;
        public const CANCELLED = 1;
        public const UNKNOWN = 2;
        public const INVALID_ARGUMENT = 3;
        public const DEADLINE_EXCEEDED = 4;
        public const NOT_FOUND = 5;
        public const ALREADY_EXISTS = 6;
        public const PERMISSION_DENIED = 7;
        public const RESOURCE_EXHAUSTED = 8;
        public const FAILED_PRECONDITION = 9;
        public const ABORTED = 10;
        public const OUT_OF_RANGE = 11;
        public const UNIMPLEMENTED = 12;
        public const INTERNAL = 13;
        public const UNAVAILABLE = 14;
        public const DATA_LOSS = 15;
        public const UNAUTHENTICATED = 16;

        private function __construct() {}

        public static function name(int $code): string {}
    }

    class StatusException extends \Exception
    {
        public readonly string $grpcMessage;
        public readonly array $metadata;

        public function __construct(int $code, string $grpcMessage = '', array $metadata = [], ?\Throwable $previous = null) {}

        public function getStatusCode(): int {}

        public function getGrpcMessage(): string {}

        public function getMetadata(): array {}
    }

    final class Context
    {
        public function method(): string {}

        public function metadata(): array {}

        public function header(string $name): ?string {}

        public function setHeader(string $name, string $value): void {}

        public function setTrailer(string $name, string $value): void {}

        public function deadlineMs(): ?int {}

        public function peer(): string {}
    }

    final class Server
    {
        public function __construct(string $address = '127.0.0.1:50051', ?\Phera\Protobuf\Schema $schema = null, array $options = []) {}

        public function unary(string $method, callable $handler, ?string $requestType = null, ?string $responseType = null): static {}

        public function serverStream(string $method, callable $handler, ?string $requestType = null, ?string $responseType = null): static {}

        public function clientStream(string $method, callable $handler, ?string $requestType = null, ?string $responseType = null): static {}

        public function enableHealthService(): static {}

        public function stop(): void {}

        public function listen(): void {}
    }

    final class Client
    {
        public function __construct(string $endpoint, ?\Phera\Protobuf\Schema $schema = null, array $options = []) {}

        public function unary(string $method, array|string $request, ?string $requestType = null, ?string $responseType = null, array $metadata = [], ?int $timeoutMs = null): array|string {}

        public function serverStream(string $method, array|string $request, ?string $requestType = null, ?string $responseType = null, array $metadata = [], ?int $timeoutMs = null): array {}

        public function clientStream(string $method, array $requests, ?string $requestType = null, ?string $responseType = null, array $metadata = [], ?int $timeoutMs = null): array|string {}

        public function lastHeaders(): array {}

        public function lastTrailers(): array {}

        public function close(): void {}
    }
}
