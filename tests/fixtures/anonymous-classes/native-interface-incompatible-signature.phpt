--TEST--
An incompatible native interface signature names the anonymous class without its location suffix
--FILE--
<?php
new class implements ArrayAccess {
    public function offsetGet(int $offset): mixed { return $offset; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetUnset(mixed $offset): void {}
};
--EXPECTF--
Fatal error: Declaration of ArrayAccess@anonymous::offsetGet(int $offset): mixed must be compatible with ArrayAccess::offsetGet(mixed $offset): mixed in %s on line 3
