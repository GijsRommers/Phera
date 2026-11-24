<?php
declare(strict_types=1);

/** @param list<array{fields?: list<int>}> $rows */
function rowTotals(array $rows): int
{
    $columns = array_map(static fn (array $row): array => $row['fields'] ?? [], $rows);
    return array_sum(array_map(static fn (array $values): int => count($values), $columns));
}
