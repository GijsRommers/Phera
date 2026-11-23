--TEST--
Array initialization respects static property types and aliases
--FILE--
<?php
class Storage {
    public static ?iterable $items;
    public static ?string $label;
}

Storage::$items['box'] = 'filled';
var_dump(Storage::$items);

try {
    Storage::$label[] = 'invalid';
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
var_dump((new ReflectionProperty(Storage::class, 'label'))->isInitialized());

$label =& Storage::$label;
try {
    $label['nested'][] = 'invalid';
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
var_dump(Storage::$label);
--EXPECT--
array(1) {
  ["box"]=>
  string(6) "filled"
}
Cannot auto-initialize an array inside property Storage::$label of type ?string
bool(false)
Cannot auto-initialize an array inside a reference held by property Storage::$label of type ?string
NULL
