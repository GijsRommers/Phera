--TEST--
A generator delegates through the Traversable cursor
--FILE--
<?php
class Entries implements Iterator {
    private int $index = 0;
    public function rewind(): void { $this->index = 0; }
    public function valid(): bool { return $this->index < 2; }
    public function current(): mixed { return 5 + $this->index; }
    public function key(): mixed { return 'value' . $this->index; }
    public function next(): void { ++$this->index; }
}
function entries() {
    $result = yield from new Entries;
    var_dump($result);
}
var_dump(iterator_to_array(entries()));
--EXPECT--
NULL
array(2) {
  ["value0"]=>
  int(5)
  ["value1"]=>
  int(6)
}
