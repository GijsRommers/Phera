--TEST--
iterator_to_array copies the keys of a nested anonymous Iterator
--FILE--
<?php
$items = new class(['a' => 'apple', 'b' => 'banana']) implements Countable, IteratorAggregate {
    public function __construct(private array $values) {}
    public function count(): int { return count($this->values); }
    public function getIterator(): Iterator {
        return new class($this->values) implements Iterator {
            private int $position = 0;
            private array $keys;
            public function __construct(private array $values) { $this->keys = array_keys($values); }
            public function rewind(): void { $this->position = 0; }
            public function valid(): bool { return $this->position < count($this->keys); }
            public function key(): mixed { return $this->keys[$this->position]; }
            public function current(): mixed { return $this->values[$this->key()]; }
            public function next(): void { ++$this->position; }
        };
    }
};
var_export(iterator_to_array($items)); echo "\n";
--EXPECT--
array (
  'a' => 'apple',
  'b' => 'banana',
)
