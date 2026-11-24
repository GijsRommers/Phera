<?php

namespace Phera\WebSocket;

/**
 * Contract implemented by application WebSocket handlers.
 *
 * Each callback executes inside its own short-lived PHP run rather than keeping
 * userland objects pinned across idle sockets.
 */
interface Handler
{
    public function onOpen(Connection $conn): void;

    public function onMessage(Connection $conn, Message $message): void;

    public function onClose(Connection $conn, int $code, string $reason): void;

    public function onError(Connection $conn, \Throwable $error): void;
}

/**
 * One complete WebSocket text or binary message delivered to Handler::onMessage().
 */
final class Message
{
    public function __construct(
        public readonly string $data,
        public readonly bool $binary = false,
    ) {
    }
}
