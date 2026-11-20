<?php
/** @template T */
class Box {}
function incomplete($value, array $items, iterable $iter): array { return $items; }
final class Incomplete
{
    public $value;
    public iterable $items;
    public function value($arg) { return $arg; }
}
function box(Box $box): Box { return $box; }
/** @phpstan-param list<string> $items */
function typedList(array $items): void {}
/** @psalm-param array<string, mixed> $items */
function typedArray(array $items): void {}
