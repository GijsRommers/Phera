--TEST--
A value getter cannot supply an array to a generator foreach by reference
--FILE--
<?php
class Items {
    public array $items { get => [3, 5]; }

    public function values(): Generator {
        foreach ($this->items as &$item) {
            yield $item;
        }
    }
}
try {
    (new Items())->values()->current();
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Indirect modification of Items::$items is not allowed
