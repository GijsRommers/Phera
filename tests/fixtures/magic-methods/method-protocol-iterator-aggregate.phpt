--TEST--
IteratorAggregate calls the declared getIterator method
--FILE--
<?php
class AggregateCursor implements Iterator {
    private bool $atStart = true;
    public function rewind(): void { $this->atStart = true; }
    public function valid(): bool { return $this->atStart; }
    public function key(): mixed { return 0; }
    public function current(): mixed { return 'value'; }
    public function next(): void { $this->atStart = false; }
}
class MagicAggregate implements IteratorAggregate {
    public function getIterator(): Traversable { echo "iterator\n"; return new AggregateCursor; }
    public function __call(string $name, array $args): mixed { echo "magic $name\n"; return null; }
}
$value = new MagicAggregate;
foreach ($value as $item) { echo "$item\n"; }
$value->missing();
--EXPECT--
iterator
value
magic missing
