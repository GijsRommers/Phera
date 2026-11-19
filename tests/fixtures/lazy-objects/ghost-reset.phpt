--TEST--
Reset clears the selected class properties and keeps the object identity
--FILE--
<?php
#[AllowDynamicProperties]
class Item { public int $quantity = 3; }
$class = new ReflectionClass(Item::class);
$item = new Item();
$item->quantity = 19;
$item->extra = 4;
$id = spl_object_id($item);
$class->resetAsLazyGhost($item, static function (Item $item) {
    echo $item->quantity, "
";
    $item->quantity = 6;
});
var_dump($id === spl_object_id($item));
echo $item->quantity, "
";
var_dump(isset($item->extra));
--EXPECT--
bool(true)
3
6
bool(false)
