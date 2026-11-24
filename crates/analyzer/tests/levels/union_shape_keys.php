<?php

/** @param array{}|array{value: string} $row */
function value(array $row): void
{
    echo $row['value'];
}

/** @param array{value?: string} $row */
function optional(array $row): void
{
    echo $row['value'];
    echo $row['value'] ?? '';
}

/** @param array{value?: ?string} $row */
function maybeNull(array $row): ?string
{
    if (array_key_exists('value', $row)) {
        return $row['value'];
    }

    return null;
}
