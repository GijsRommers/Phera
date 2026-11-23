--TEST--
An anonymous IteratorAggregate returns an anonymous Iterator
--FILE--
<?php
$items = new class implements IteratorAggregate {
    public function getIterator(): Iterator {
        return new class implements Iterator {
            private int $position = 0;
            public function rewind(): void { $this->position = 0; }
            public function valid(): bool { return $this->position < 2; }
            public function current(): mixed { return ['a', 'b'][$this->position]; }
            public function key(): mixed { return $this->position; }
            public function next(): void { ++$this->position; }
        };
    }
};
foreach ($items as $value) { echo $value; }
echo "\n";
--EXPECT--
ab
