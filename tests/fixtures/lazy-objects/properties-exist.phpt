--TEST--
property_exists checks the declaration before initializing a dynamic lookup
--FILE--
<?php
class Entry { public int $id; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) { echo "initializer
"; });
var_dump(property_exists($entry, 'id'));
var_dump(property_exists($entry, 'missing'));
--EXPECT--
bool(true)
initializer
bool(false)
