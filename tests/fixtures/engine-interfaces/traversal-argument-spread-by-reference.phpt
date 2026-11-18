--TEST--
traversal argument spread by reference
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
$it->keyValue=0; function f(&$a,$b) {var_dump($a,$b);} f(...$it);
--EXPECTF--
rewind
valid
current
key

Warning: Cannot pass by-reference argument 1 of f() by unpacking a Traversable, passing by-value instead in %s on line 18
next
valid
current
key
next
valid
int(11)
int(12)
