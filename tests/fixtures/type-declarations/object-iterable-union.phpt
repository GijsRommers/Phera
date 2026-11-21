--TEST--
Object and iterable can form a union and share a property contract with object and array
--FILE--
<?php
class ParentType {
    public object|iterable $value;
}
class ChildType extends ParentType {
    public object|array $value;
}
function acceptValue(object|iterable|null $value): void {
    echo get_debug_type($value), "\n";
}
$child = new ChildType();
$child->value = [];
acceptValue($child->value);
acceptValue(new stdClass());
acceptValue(null);
--EXPECT--
array
stdClass
null
