<?php
declare(strict_types=1);

/** @template T */
interface FixtureRows
{
    /** @return list<T> */
    public function toArray(): array;
}

final class FixtureRow
{
    public function label(): string { return 'row'; }
    public function visible(): bool { return random_int(0, 1) === 1; }
}

/**
 * @param FixtureRows<FixtureRow> $rows
 * @return array<int, string>
 */
function labelsOfRows(FixtureRows $rows): array
{
    return array_map(static fn ($row) => $row->label(), array_filter($rows->toArray(), static fn (FixtureRow $row): bool => $row->visible()));
}
