<?php
declare(strict_types=1);

function queryRows(): mixed
{
    return [['id' => 1]];
}

/** @return list<array{id: int}> */
function rows(): array
{
    /** @var list<array{id: int}> */
    return queryRows();
}

function scalarRow(): string
{
    $value = queryRows();
    /** @phpstan-var string */
    return $value;
}
