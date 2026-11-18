--TEST--
arrayaccess missing empty
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
$a->exists = false; var_dump(empty($a[1]));
--EXPECT--
exists int(1)
bool(true)
