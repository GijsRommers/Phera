--TEST--
Property reference holders must agree on how to convert the original input
--FILE--
<?php
class Item {
    public string|int $count = 'before';
    public string|float $description;
}
$item = new Item;
$ref =& $item->count;
$item->description =& $ref;
try {
    $ref = 42;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($ref, $item->description);
--EXPECT--
Cannot assign int to reference held by property Item::$count of type string|int and property Item::$description of type string|float, as this would result in an inconsistent type conversion
string(6) "before"
string(6) "before"
