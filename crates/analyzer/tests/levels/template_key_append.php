<?php

/**
 * @template K of array-key
 * @template V
 */
final class Store
{
    /** @var array<K, V> */
    private array $entries = [];

    /** @param V $entry */
    public function add(mixed $entry): void
    {
        $this->entries[] = $entry;
    }

    /** @return array<K, V> */
    public function entries(): array
    {
        return $this->entries;
    }
}
