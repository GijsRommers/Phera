--TEST--
An enum case does not dispatch ArrayAccess in a constant expression
--FILE--
<?php
enum Config implements ArrayAccess {
    case Item;
    public function offsetGet(mixed $offset): mixed { echo "called\n"; return 1; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetUnset(mixed $offset): void {}
}
class Options {
    const Item = Config::Item[Config::Item];
}
try {
    echo Options::Item;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Cannot use [] on objects in constant expression
