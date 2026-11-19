--TEST--
JSON initializes ghosts reached during an array walk
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) {
    echo "initializer
";
    $entry->id = 9;
});
echo json_encode(['entry' => $entry]), "
";
--EXPECT--
initializer
{"entry":{"id":9}}
