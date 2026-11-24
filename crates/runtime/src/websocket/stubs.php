<?php

/**
 * Native signature declarations for Phera\WebSocket, checked by
 * crates/runtime/src/native_parameter_tests.rs.
 */

namespace Phera\WebSocket;

function accept(string $handler, array $context = [], array $rooms = [], ?string $subprotocol = null): void {}

function broadcast(string $room, string $data, bool $binary = false): int {}

function send(string $connectionId, string $data, bool $binary = false): bool {}

function roomCount(string $room): int {}

function connections(string $room): array {}

function resolveUsing(callable $resolver): void {}

function isEvent(): bool {}

final class Connection
{
    private function __construct() {}

    public function id(): string {}

    public function context(): mixed {}

    public function setContext(mixed $context): void {}

    public function send(string $data, bool $binary = false): bool {}

    public function sendTo(string $connectionId, string $data, bool $binary = false): bool {}

    public function broadcast(string $room, string $data, bool $binary = false, bool $includeSelf = false): int {}

    public function join(string $room): bool {}

    public function leave(string $room): bool {}

    public function rooms(): array {}

    public function close(int $code = 1000, string $reason = ''): bool {}
}
