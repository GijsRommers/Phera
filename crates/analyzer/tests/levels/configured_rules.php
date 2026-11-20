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
