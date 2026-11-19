--TEST--
Proxy property reads and writes use a distinct backing object
--FILE--
<?php
class Item { public int $quantity = 3; }
$class = new ReflectionClass(Item::class);
$real = new Item();
$proxy = $class->newLazyProxy(static fn () => $real);
echo $proxy->quantity, "
";
$proxy->quantity = 9;
echo $real->quantity, "
";
var_dump($class->initializeLazyObject($proxy) === $real);
var_dump(spl_object_id($proxy) !== spl_object_id($real));
--EXPECT--
3
9
bool(true)
bool(true)
