<?php
declare(strict_types=1);

/**
 * @template T
 * @param Closure(): T $callback
 * @return T
 */
function runCallback(Closure $callback): mixed
{
    return $callback();
}

function knownArrow(): int
{
    return (int) runCallback(static fn (): mixed => 1);
}

function unknownArrow(mixed $value): int
{
    return (int) runCallback(static fn (): mixed => $value);
}
