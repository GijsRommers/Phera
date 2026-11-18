--TEST--
arrayaccess nested write
--FILE--
<?php
class Store implements ArrayAccess {
    public $value = 5;
    public $exists = true;
    public function offsetExists(mixed $offset): bool { echo "exists "; var_dump($offset); return $this->exists; }
    public function offsetGet(mixed $offset): mixed { echo "get "; var_dump($offset); return $this->value; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "set "; var_dump($offset, $value); $this->value = $value; }
    public function offsetUnset(mixed $offset): void { echo "unset "; var_dump($offset); }
}
$a = new Store;
$a->value = ["b"=>3]; $a["a"]["b"] = 9; var_dump($a->value);
--EXPECTF--
get string(1) "a"

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 11
array(1) {
  ["b"]=>
  int(3)
}
