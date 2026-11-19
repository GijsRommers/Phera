--TEST--
Foreach by reference binds the backing instance's property
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static function () {
    echo "factory
";
    return new Entry();
});
foreach ($entry as $key => &$value) {
    echo "$key=$value
";
    $value = 9;
}
unset($value);
echo $class->initializeLazyObject($entry)->id, "
";
--EXPECT--
factory
id=5
9
