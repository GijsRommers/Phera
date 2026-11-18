--TEST--
ArrayAccess forwards the raw offset into a property array write
--FILE--
<?php
class Box implements ArrayAccess {
    public array $data = [];
    public function offsetSet(mixed $offset, mixed $value): void {
        echo "set\n";
        $this->data[$offset] = $value;
    }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetUnset(mixed $offset): void {}
}
$b = new Box;
$b[1.7] = 'v';
var_dump($b->data);
--EXPECTF--
set

Deprecated: Implicit conversion from float 1.7 to int loses precision in /private%s on line 6
array(1) {
  [1]=>
  string(1) "v"
}
