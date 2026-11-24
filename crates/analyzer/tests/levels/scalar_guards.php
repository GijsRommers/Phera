<?php

function printable(mixed $value): string
{
    if (!is_scalar($value)) {
        return '';
    }
    return (string) $value;
}

/** @param array<string, mixed> $row */
function readNumber(array $row, string $key): int
{
    return is_numeric($row[$key] ?? null) ? (int) $row[$key] : 0;
}

/** @param array{count: mixed} $row */
function readEitherNumber(array $row): float
{
    if (is_int($row['count']) || is_float($row['count'])) {
        return (float) $row['count'];
    }
    return 0.0;
}

function positiveId(?int $id): int
{
    if (($id ?? 0) > 0) {
        return $id;
    }
    return 0;
}

enum Mode
{
    case Active;
    case Inactive;
}

final class Connection
{
    public function __construct(public Mode $mode, public int $id) {}
}

function activeConnection(?Connection $connection): int
{
    if ($connection?->mode === Mode::Active) {
        return $connection->id;
    }
    return 0;
}
