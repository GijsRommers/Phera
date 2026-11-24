<?php
declare(strict_types=1);

function readClosure(Closure $callback): string
{
    return $callback();
}

function readCallable(callable $callback): string
{
    return $callback();
}

/** @return Closure(): string */
function createTyped(): Closure
{
    return fn (): string => 'text';
}

function readTyped(): string
{
    $callback = createTyped();

    return $callback();
}

function readOptional(?Closure $callback): string
{
    if (!$callback) {
        $callback = static fn (): string => 'default';
    }

    return $callback();
}
