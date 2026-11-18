--TEST--
arrayaccess indirect append
--FILE--
<?php
class NestedStore implements ArrayAccess {
    private $items = [];
    public function &offsetGet(mixed $offset): mixed {
        if ($offset === null) return $this->items[];
        return $this->items[$offset];
    }
    public function offsetExists(mixed $offset): bool { return isset($this->items[$offset]); }
    public function offsetSet(mixed $offset, mixed $value): void { $this->items[$offset]=$value; }
    public function offsetUnset(mixed $offset): void { unset($this->items[$offset]); }
    public function items(): array { return $this->items; }
}
$store = new NestedStore;
$store[3] = 1;
$store[][][0] = 2;
var_dump($store->items());
--EXPECT--
array(2) {
  [3]=>
  int(1)
  [4]=>
  array(1) {
    [0]=>
    array(1) {
      [0]=>
      int(2)
    }
  }
}
