<?php

final class OffsetRow
{
    public function __construct(public readonly string $label) {}
}

function displayRow(string $label): void {}

function useOffsets(mixed $source, $position): void
{
    displayRow($source->rows[0]);
    displayRow($source->resolve()->label);
    displayRow($source[0][0]);
    $source->rows[$position - 1][] = $source->rows[$position - 1];
    $source[] = 1;
    $source->resolve($source->rows[$position], $source->rows[$position]);

    if ($source->rows[$position - (1 - 1)] instanceof OffsetRow) {
        displayRow($source->rows[$position - (1 - 1)]->label);
    } elseif ($source->rows[$position - (1 - 1)] === null) {
        $source->rows[$position - (1 - 1)] = '';
    } else {
        displayRow($source->rows[$position - (1 - 1)]);
    }

    foreach ($source->rows[$position - (1 - 1)] as $row) {}
    displayRow($source->rows[$position - (1 - 1)]);
    displayRow($source->rows[$position - 0]);
}
