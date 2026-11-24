<?php

function printParts(string $pattern, string $line): void
{
    foreach (preg_split($pattern, $line) as $part) {
        echo $part;
    }

    $parts = preg_split($pattern, $line, -1, PREG_SPLIT_NO_EMPTY);
    if ($parts === false) {
        return;
    }
    foreach ($parts as $part) {
        echo $part;
    }
}
