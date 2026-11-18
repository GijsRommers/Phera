--TEST--
A trait supplies the methods for Iterator
--FILE--
<?php
trait IteratorMethods {
    private int $position = 0;
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 2; }
    public function current(): mixed { return $this->position * 10; }
    public function key(): mixed { return $this->position; }
    public function next(): void { ++$this->position; }
}
class Items implements Iterator { use IteratorMethods; }
foreach (new Items as $key => $value) {
    echo $key, ':', $value, "\n";
}
--EXPECT--
0:0
1:10
