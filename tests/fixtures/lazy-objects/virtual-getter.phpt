--TEST--
A virtual getter can return without initializing backing storage
--FILE--
<?php
class Entry {
    public int $id;
    public string $label { get => 'record'; }
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) { $entry->id = 7; });
echo $entry->label, "
";
var_dump($class->isUninitializedLazyObject($entry));
echo $entry->id, "
";
--EXPECT--
record
bool(true)
7
