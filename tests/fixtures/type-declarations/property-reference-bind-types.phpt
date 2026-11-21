--TEST--
Binding to an existing typed alias cannot change its type
--FILE--
<?php
class Item {
    public int $count;
    public string $text;
}
$item = new Item;
$source = "42";
$item->count =& $source;
var_dump($source);
try {
    $item->text =& $item->count;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($item->count, isset($item->text));
--EXPECT--
int(42)
Reference with value of type int held by property Item::$count of type int is not compatible with property Item::$text of type string
int(42)
bool(false)
