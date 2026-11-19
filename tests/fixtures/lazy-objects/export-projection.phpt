--TEST--
var_export reads the proxy's backing properties
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static function () {
    echo "factory
";
    return new Entry();
});
echo var_export($entry, true), "
";
--EXPECT--
factory
\Entry::__set_state(array(
   'id' => 5,
))
