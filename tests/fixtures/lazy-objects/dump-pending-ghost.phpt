--TEST--
var_dump lists lazy typed slots without invoking the ghost initializer
--FILE--
<?php
class Entry {
    public int $id;
    public string $label;
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function () { echo "initializer
"; });
$class->getProperty('id')->setRawValueWithoutLazyInitialization($entry, 7);
var_dump($entry);
var_dump($class->isUninitializedLazyObject($entry));
--EXPECTF--
lazy ghost object(Entry)#%d (1) {
  ["id"]=>
  int(7)
  ["label"]=>
  uninitialized(string)
}
bool(true)
