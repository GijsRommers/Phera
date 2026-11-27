<?php

declare(strict_types=1);

namespace Phera\Psr;

use Phera\Log\Logger;
use Psr\Log\InvalidArgumentException;
use Psr\Log\LoggerInterface;

/**
 * `Phera\Log\Logger` as a PSR-3 logger, for libraries and frameworks that
 * take a `Psr\Log\LoggerInterface`:
 *
 * ```php
 * $logger = new Psr3Logger(new Logger('app', 'info', 'json', '/var/log/app.log'));
 * ```
 *
 * Every call goes straight to the native logger, which drops records
 * below its level before looking at the message or the context.
 */
final class Psr3Logger implements LoggerInterface
{
    public function __construct(private readonly Logger $logger = new Logger()) {}

    /** The native logger underneath. */
    public function logger(): Logger
    {
        return $this->logger;
    }

    /** A PSR-3 logger to the same target with `$context` added to every record. */
    public function withContext(array $context): self
    {
        return new self($this->logger->withContext($context));
    }

    public function emergency(string|\Stringable $message, array $context = []): void
    {
        $this->logger->emergency((string) $message, $context);
    }

    public function alert(string|\Stringable $message, array $context = []): void
    {
        $this->logger->alert((string) $message, $context);
    }

    public function critical(string|\Stringable $message, array $context = []): void
    {
        $this->logger->critical((string) $message, $context);
    }

    public function error(string|\Stringable $message, array $context = []): void
    {
        $this->logger->error((string) $message, $context);
    }

    public function warning(string|\Stringable $message, array $context = []): void
    {
        $this->logger->warning((string) $message, $context);
    }

    public function notice(string|\Stringable $message, array $context = []): void
    {
        $this->logger->notice((string) $message, $context);
    }

    public function info(string|\Stringable $message, array $context = []): void
    {
        $this->logger->info((string) $message, $context);
    }

    public function debug(string|\Stringable $message, array $context = []): void
    {
        $this->logger->debug((string) $message, $context);
    }

    public function log($level, string|\Stringable $message, array $context = []): void
    {
        if (!is_string($level) && !$level instanceof \Stringable) {
            throw new InvalidArgumentException('A log level must be a string, ' . get_debug_type($level) . ' given');
        }
        try {
            $this->logger->log((string) $level, (string) $message, $context);
        } catch (\InvalidArgumentException $exception) {
            throw new InvalidArgumentException($exception->getMessage(), 0, $exception);
        }
    }
}
