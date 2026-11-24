<?php
declare(strict_types=1);

function storeRequired(string $item): void {}

/** @param list<string> $items */
function takeTwo(array $items): void
{
    if (count($items) < 2) {
        return;
    }

    storeRequired(array_pop($items));
    storeRequired(array_pop($items));
    storeRequired(array_pop($items));
}
