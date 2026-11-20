<?php

function append(string $buffer, mixed $value): string
{
    $buffer .= $value;
    ++$value;

    return $buffer;
}

/** @param array<array-key, string> $lookup */
function lookup(array $lookup, mixed $key): string
{
    return $lookup[$key];
}
