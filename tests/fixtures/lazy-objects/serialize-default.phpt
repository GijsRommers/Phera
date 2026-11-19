--TEST--
Serialization initializes a nested ghost when it reaches its properties
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) {
    echo "initializer
";
    $entry->id = 9;
});
echo serialize([$entry]), "
";
--EXPECT--
initializer
a:1:{i:0;O:5:"Entry":1:{s:2:"id";i:9;}}
