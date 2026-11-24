<?php
final class Item { public function name(): string { return 'item'; } }
final class Store
{
    private ?Item $item = null;

    public function get(): string
    {
        $this->init();

        return $this->item->name();
    }

    /** @phpstan-assert Item $this->item */
    private function init(): void
    {
        $this->item ??= new Item();
    }
}
/**
 * @template T of object
 * @param class-string<T> $class
 * @return T
 */
function create(string $class): object
{
    return new $class();
}
/**
 * @phpstan-param array{name: non-empty-string} $row
 * @psalm-param callable(non-empty-string): Item $factory
 */
function map(array $row, callable $factory): Item
{
    return $factory($row['name']);
}
