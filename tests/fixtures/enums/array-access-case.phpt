--TEST--
ArrayAccess can dispatch reads on an enum case
--FILE--
<?php
enum Config implements ArrayAccess {
    case Production;
    public function offsetExists(mixed $offset): bool { return $offset === 'env'; }
    public function offsetGet(mixed $offset): mixed { return $offset === 'env' ? $this->name : null; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
$case = Config::Production;
var_dump(isset($case['env']), $case['env']);
--EXPECT--
bool(true)
string(10) "Production"
