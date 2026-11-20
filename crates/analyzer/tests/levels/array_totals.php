<?php
declare(strict_types=1);

/** @param list<int> $values */
function intTotal(array $values): int
{
    return array_sum($values);
}

/** @param list<float> $values */
function floatTotal(array $values): float
{
    return array_sum($values);
}
