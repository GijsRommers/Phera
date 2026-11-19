--TEST--
Cloning a proxy clones its backing instance and calls __clone once
--FILE--
<?php
class Entry {
    public int $id = 5;
    public function __clone() { echo "cloned
"; }
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static function () {
    echo "factory
";
    return new Entry();
});
$copy = clone $entry;
$copy->id = 6;
var_dump($entry->id, $copy->id);
var_dump($class->initializeLazyObject($entry) !== $class->initializeLazyObject($copy));
--EXPECT--
factory
cloned
int(5)
int(6)
bool(true)
