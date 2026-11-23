--TEST--
get_object_vars initializes the backing instance before listing its properties
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static function () {
    echo "factory
";
    return new Entry();
});
var_dump(get_object_vars($entry));
--EXPECT--
factory
array(1) {
  ["id"]=>
  int(5)
}
