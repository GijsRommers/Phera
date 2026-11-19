--TEST--
Marking a ghost initialized restores defaults without calling its initializer
--FILE--
<?php
class Item { public int $count = 3; }
$class = new ReflectionClass(Item::class);
$initializer = static function (Item $self) { echo "initializer
"; };
$item = $class->newLazyGhost($initializer);
var_dump($class->getLazyInitializer($item) === $initializer);
var_dump($class->markLazyObjectAsInitialized($item) === $item);
var_dump($class->getLazyInitializer($item));
echo $item->count, "
";
--EXPECT--
bool(true)
bool(true)
NULL
3
