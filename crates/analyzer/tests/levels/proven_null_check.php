<?php

/** @param array{label?: string} $entry */
function label(array $entry): string
{
    return $entry['name'] ?? '';
}

final class NumberNode {}

function build(NumberNode|string $node): void
{
    if (!is_string($node) && !$node instanceof NumberNode) { // @phpstan-ignore instanceof.alwaysTrue
        throw new LogicException();
    }
}
