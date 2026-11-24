<?php

final class ChainRow
{
    public int $id = 4;
    public function id(): int { return $this->id; }
}

final class ChainQuery
{
    public function row(): ChainRow { return new ChainRow(); }
    public function optionalRow(bool $flag): ?ChainRow { return $flag ? new ChainRow() : null; }

    /** @return array{row: ChainRow} */
    public function rows(): array { return ['row' => new ChainRow()]; }
}

function idFromRow(?ChainQuery $query): ?int
{
    return $query?->row()->id();
}

function idFromOffset(?ChainQuery $query): ?int
{
    return $query?->rows()['row']->id;
}

function idFromOptionalRow(?ChainQuery $query, bool $flag): ?int
{
    return $query?->optionalRow($flag)->id();
}
