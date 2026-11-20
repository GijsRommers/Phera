<?php

function renderValue(): string
{
    $unused = strval(random_int(0, 10));

    return 'ok';
}

/** @param list<string> $items */
function writeItems(array $items): void
{
    foreach ($items as $position => $item) {
        echo $item;
    }
}
