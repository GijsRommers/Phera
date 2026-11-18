--TEST--
ArrayAccess uses the declared offset methods even on objects with magic methods
--FILE--
<?php
class MagicOffsets implements ArrayAccess {
    public function offsetGet(mixed $offset): mixed { echo "get $offset\n"; return 'value'; }
    public function offsetSet(mixed $offset, mixed $value): void { echo "set $offset $value\n"; }
    public function offsetExists(mixed $offset): bool { echo "exists $offset\n"; return true; }
    public function offsetUnset(mixed $offset): void { echo "unset $offset\n"; }
    public function __call(string $name, array $args): mixed { echo "magic $name\n"; return null; }
}
$value = new MagicOffsets;
var_dump($value['key'], isset($value['key']), empty($value['key']));
$input = 1;
var_dump($value['key'] = $input);
unset($value['key']);
$value->missing();
--EXPECT--
get key
exists key
exists key
get key
string(5) "value"
bool(true)
bool(false)
set key 1
int(1)
unset key
magic missing
