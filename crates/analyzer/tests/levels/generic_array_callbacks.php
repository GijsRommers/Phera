<?php
declare(strict_types=1);

/** @template T */
interface FixtureRows
{
    /** @return list<T> */
    public function toArray(): array;
}

/**
 * @phpstan-template U
 * @template-extends FixtureRows<U>
 */
interface ExtendedRows extends FixtureRows {}

final class FixtureRow
{
    public function label(): string { return 'row'; }
    public function visible(): bool { return random_int(0, 1) === 1; }
}

/**
 * @param ExtendedRows<FixtureRow> $rows
 * @return array<int, string>
 */
function labelsOfRows(ExtendedRows $rows): array
{
    return array_map(static fn ($row) => $row->label(), array_filter($rows->toArray(), static fn (FixtureRow $row): bool => $row->visible()));
}
