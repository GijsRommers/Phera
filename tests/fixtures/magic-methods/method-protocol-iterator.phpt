--TEST--
Iteration dispatches the methods declared for Iterator even with __call and __invoke
--FILE--
<?php
class MagicCursor implements Iterator {
    private int $position = 0;
    public function rewind(): void { echo "rewind\n"; $this->position = 0; }
    public function valid(): bool { echo "valid\n"; return $this->position < 1; }
    public function key(): mixed { echo "key\n"; return 2; }
    public function current(): mixed { echo "current\n"; return 3; }
    public function next(): void { echo "next\n"; ++$this->position; }
    public function __call(string $name, array $args): mixed { echo "magic $name\n"; return null; }
    public function __invoke() { echo "invoke\n"; }
}
$cursor = new MagicCursor;
foreach ($cursor as $key => $value) { echo "$key=$value\n"; }
$cursor->missing();
$cursor();
--EXPECT--
rewind
valid
current
key
2=3
next
valid
magic missing
invoke
