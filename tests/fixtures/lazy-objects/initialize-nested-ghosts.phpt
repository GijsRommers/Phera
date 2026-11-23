--TEST--
Nested ghost reads suspend PHP frames until their initializers return
--FILE--
<?php
class Item { public int $quantity; }
$class = new ReflectionClass(Item::class);
$inner = $class->newLazyGhost(static function (Item $item) { $item->quantity = 41; });
$outer = $class->newLazyGhost(static function (Item $item) use ($inner) {
    $item->quantity = $inner->quantity + 1;
});
echo $outer->quantity, "
";
--EXPECT--
42
