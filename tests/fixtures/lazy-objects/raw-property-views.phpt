--TEST--
Array casts and mangled-property views skip initialization
--FILE--
<?php
class Entry {
    public int $id;
    private string $label;
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function () { echo "initializer
"; });
$class->getProperty('id')->setRawValueWithoutLazyInitialization($entry, 7);
var_dump((array) $entry, get_mangled_object_vars($entry));
var_dump($class->isUninitializedLazyObject($entry));
--EXPECT--
array(1) {
  ["id"]=>
  int(7)
}
array(1) {
  ["id"]=>
  int(7)
}
bool(true)
