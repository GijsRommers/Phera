--TEST--
arrayaccess reference binding forbidden
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
$value=5; try {$a[1]=&$value;} catch(Error $e) {echo $e->getMessage(),"\n";}
--EXPECTF--
get int(1)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 11
Cannot assign by reference to an array dimension of an object
