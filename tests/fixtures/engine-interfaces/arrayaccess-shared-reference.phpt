--TEST--
arrayaccess shared reference
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
$store['group'] = ['key' => 1];
$one = &$store['group']['key'];
$two = &$store['group']['key'];
$one = 6;
var_dump($two, $store['group']['key']);
--EXPECT--
int(6)
int(6)
