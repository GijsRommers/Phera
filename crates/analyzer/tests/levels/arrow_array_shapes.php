<?php
declare(strict_types=1);

/** @param list<int> $ids */
function sumRowIds(array $ids): int
{
    $rows = array_map(static fn (int $id): array => ['id' => $id], $ids);
    return array_sum(array_column($rows, 'id'));
}
