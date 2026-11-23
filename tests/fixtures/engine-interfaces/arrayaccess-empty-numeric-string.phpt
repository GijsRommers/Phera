--TEST--
arrayaccess empty numeric string
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
var_dump(empty($a['7']));
--EXPECT--
exists string(1) "7"
get string(1) "7"
bool(false)
