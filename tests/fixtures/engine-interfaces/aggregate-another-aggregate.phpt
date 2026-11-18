--TEST--
aggregate another aggregate
--FILE--
<?php
class Items implements Iterator {
    public $keyValue = "a";
    public $throwAt = "";
    public $position = 0;
    public $size = 2;
    private function observe(string $name): void {
        echo $name, "\n";
        if ($this->throwAt === $name) { throw new Exception($name); }
    }
    public function rewind(): void { $this->observe("rewind"); $this->position = 0; }
    public function valid(): bool { $this->observe("valid"); return $this->position < $this->size; }
    public function current(): mixed { $this->observe("current"); return $this->position + 11; }
    public function key(): mixed { $this->observe("key"); return $this->position === 0 ? $this->keyValue : 2; }
    public function next(): void { $this->observe("next"); ++$this->position; }
}
$it = new Items;

class Aggregate implements IteratorAggregate {
    public Traversable $it; public function __construct(Traversable $it) { $this->it = $it; }
    public function getIterator(): Traversable { echo "getIterator\n"; return $this->it; }
}
foreach (new Aggregate(new Aggregate($it)) as $v) { echo $v, "\n"; }
--EXPECT--
getIterator
getIterator
rewind
valid
current
11
next
valid
current
12
next
valid
