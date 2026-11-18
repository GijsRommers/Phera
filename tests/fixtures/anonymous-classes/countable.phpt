--TEST--
Countable anonymous classes dispatch count to the method
--FILE--
<?php
$counter = new class(3) implements Countable {
    public function __construct(private int $items) {}
    public function count(): int { return $this->items; }
};
var_dump(count($counter));
--EXPECT--
int(3)
