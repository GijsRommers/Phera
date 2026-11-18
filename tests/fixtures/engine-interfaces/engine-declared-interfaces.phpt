--TEST--
engine declared interfaces
--FILE--
<?php
$interfaces = get_declared_interfaces();
foreach (['ArrayAccess','Countable','Iterator','IteratorAggregate','Traversable','JsonSerializable'] as $name) {
    var_dump(in_array($name, $interfaces, true), interface_exists($name, false), class_exists($name, false));
}
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
