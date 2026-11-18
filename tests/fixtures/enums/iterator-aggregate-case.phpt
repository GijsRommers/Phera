--TEST--
Foreach calls the iterator returned by an IteratorAggregate enum case
--FILE--
<?php
enum Days implements IteratorAggregate {
    case Weekend;
    public function getIterator(): Iterator {
        return new class($this->name) implements Iterator {
            private int $position = 0;
            public function __construct(private string $name) {}
            public function rewind(): void { $this->position = 0; }
            public function valid(): bool { return $this->position < 1; }
            public function current(): mixed { return $this->name; }
            public function key(): mixed { return 'day'; }
            public function next(): void { ++$this->position; }
        };
    }
}
foreach (Days::Weekend as $key => $day) {
    echo $key, '=', $day, "\n";
}
--EXPECT--
day=Weekend
