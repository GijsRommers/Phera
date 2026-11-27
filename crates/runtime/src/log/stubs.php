<?php

/**
 * Native signature declarations for Phera\Log, checked by
 * crates/runtime/src/native_parameter_tests.rs. Also usable as IDE stubs.
 */

namespace Phera\Log;

/** The PSR-3 level names, as `Psr\Log\LogLevel` has them. */
final class Level
{
    public const EMERGENCY = 'emergency';
    public const ALERT = 'alert';
    public const CRITICAL = 'critical';
    public const ERROR = 'error';
    public const WARNING = 'warning';
    public const NOTICE = 'notice';
    public const INFO = 'info';
    public const DEBUG = 'debug';

    private function __construct() {}
}

/**
 * A structured logger with the PSR-3 methods. `$format` is `json` (one
 * object per line) or `text`; `$target` is `stderr`, `stdout`, `memory` or
 * a file path (appended to by a writer thread). Records below `$level` are
 * dropped before anything else happens.
 */
final class Logger
{
    public function __construct(string $channel = 'app', string $level = 'info', string $format = 'json', string $target = 'stderr', array $defaultContext = []) {}

    /** A logger that keeps its records for `records()`, for tests. */
    public static function memory(string $channel = 'app', string $level = 'debug', string $format = 'json'): static {}

    public function emergency(string $message, array $context = []): void {}

    public function alert(string $message, array $context = []): void {}

    public function critical(string $message, array $context = []): void {}

    public function error(string $message, array $context = []): void {}

    public function warning(string $message, array $context = []): void {}

    public function notice(string $message, array $context = []): void {}

    public function info(string $message, array $context = []): void {}

    public function debug(string $message, array $context = []): void {}

    /** @throws \InvalidArgumentException for a level that is not one of the eight */
    public function log(string $level, string $message, array $context = []): void {}

    /** Whether a record at `$level` would be written. */
    public function isHandling(string $level): bool {}

    /** A logger to the same target with `$context` merged into its default context. */
    public function withContext(array $context): static {}

    /** Waits until every record so far is written. */
    public function flush(): void {}

    /**
     * The records of a `memory` logger, oldest first: each has `ts`, `level`,
     * `channel`, `message` (placeholders filled in), `context` and `formatted`.
     */
    public function records(): array {}
}
