--TEST--
An uninitialized iterable property accepts array promotion
--FILE--
<?php
class Item {
    public ?iterable $entries;
}
$item = new Item;
$item->entries[] = 7;
var_dump($item->entries);
--EXPECT--
array(1) {
  [0]=>
  int(7)
}
