--TEST--
ReflectionProperty::getRawValue initializes a lazy backing slot
--FILE--
<?php
class Entry { public int $id; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) {
    echo "initializer
";
    $entry->id = 8;
});
var_dump($class->getProperty('id')->getRawValue($entry));
--EXPECT--
initializer
int(8)
