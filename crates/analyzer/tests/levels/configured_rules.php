<?php

/**
 * @param array{email: ?string, display?: ?string} $record
 * @param list<string> $names
 */
function applyRecord(array $record, array $names): ?string
{
    $key = null;
    $value = null;
    echo $key, $value;
    foreach ($names as $key => $value) {
        echo $key, $value;
    }

    $display = $record['display'] ?? null;
    return $display ?? ($record['email'] ?? null);
}

/** @param list<array{email: ?string, display?: ?string}> $records */
function emails(array $records): array
{
    return array_map(static fn(array $record): ?string => $record['email'] ?? null, $records);
}

/** @param list<string> $names */
function copiedAccumulator(array $names): void
{
    $filtered = [];
    foreach ($names as $name) {
        if ($name !== '') {
            $filtered[] = $name;
        }
    }
    $copy = $filtered;
    if ($copy === []) {
        return;
    }
    foreach ($copy as $name) {
        echo $name;
    }
}

/** @param non-empty-list<string> $names */
function nonemptyIterable(array $names): void
{
    $filtered = [];
    foreach ($names as $name) {
        if ($name !== '') {
            $filtered[] = $name;
        }
    }
    if ($filtered === []) {
        return;
    }
    foreach ($filtered as $name) {
        echo $name;
    }
}
