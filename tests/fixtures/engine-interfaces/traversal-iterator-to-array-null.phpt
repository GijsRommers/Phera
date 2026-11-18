--TEST--
traversal iterator to array null
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
$it->keyValue = null; var_dump(iterator_to_array($it));
--EXPECTF--
rewind
valid
current
key

Deprecated: Using null as an array offset is deprecated, use an empty string instead in %s on line 18
next
valid
current
key
next
valid
array(2) {
  [""]=>
  int(11)
  [2]=>
  int(12)
}
