--TEST--
A trait supplies the methods for ArrayAccess
--FILE--
<?php
trait ArrayMethods {
    private array $items = [];
    public function offsetExists(mixed $offset): bool { return isset($this->items[$offset]); }
    public function offsetGet(mixed $offset): mixed { return $this->items[$offset]; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->items[$offset] = $value; }
    public function offsetUnset(mixed $offset): void { unset($this->items[$offset]); }
}
class Storage implements ArrayAccess { use ArrayMethods; }
$storage = new Storage;
$storage['a'] = 'value';
var_dump(isset($storage['a']), $storage['a']);
unset($storage['a']);
var_dump(isset($storage['a']));
--EXPECT--
bool(true)
string(5) "value"
bool(false)
