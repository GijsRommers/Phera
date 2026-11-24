<?php
declare(strict_types=1);

/**
 * @param list<array{rows: list<array{id: int}>}> $batches
 * @return list<int>
 */
function mergedRowIds(array $batches): array
{
    $rows = array_merge(...array_map(static fn (array $batch): array => $batch['rows'], $batches));
    return array_map(static fn ($row): int => $row['id'], $rows);
}
