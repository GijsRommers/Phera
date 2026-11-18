--TEST--
ArrayAccess receives the enum case itself as the offset
--FILE--
<?php
enum Option { case Verbose; }
$config = new class implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return $offset instanceof Option; }
    public function offsetGet(mixed $offset): mixed {
        echo $offset->name, "\n";
        return $offset === Option::Verbose;
    }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
};
var_dump($config[Option::Verbose]);
--EXPECT--
Verbose
bool(true)
