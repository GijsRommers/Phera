<?php

class Entry {}

/**
 * @template T of Entry
 * @param class-string<T> $class
 * @return T
 */
function readEntry(string $class): Entry
{
    return new Entry();
}

/** @param array<class-string, string> $map */
function entryName(array $map, ?string $class): string
{
    return $map[$class];
}
