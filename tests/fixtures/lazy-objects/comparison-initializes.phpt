--TEST--
Nested loose comparison initializes the compared ghosts in operand order
--FILE--
<?php
class Entry { public int $id; }
$class = new ReflectionClass(Entry::class);
$first = $class->newLazyGhost(static function (Entry $entry) {
    echo "first
";
    $entry->id = 7;
});
$second = $class->newLazyGhost(static function (Entry $entry) {
    echo "second
";
    $entry->id = 7;
});
var_dump($first === $second);
var_dump([12, $first] == [13, $second]);
var_dump([12, $first] == [12, $second]);
--EXPECT--
bool(false)
bool(false)
first
second
bool(true)
