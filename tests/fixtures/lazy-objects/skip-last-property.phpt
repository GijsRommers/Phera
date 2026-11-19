--TEST--
Skipping the last lazy property discards the initializer
--FILE--
<?php
class Item { public int $quantity = 3; }
$class = new ReflectionClass(Item::class);
$item = $class->newLazyGhost(static function () { echo "unexpected
"; });
$property = new ReflectionProperty(Item::class, 'quantity');
$property->skipLazyInitialization($item);
var_dump($property->isLazy($item), $class->isUninitializedLazyObject($item));
echo $item->quantity, "
";
--EXPECT--
bool(false)
bool(false)
3
