--TEST--
Acquiring an uninitialized non-nullable property reports its anonymous class name
--FILE--
<?php
$item = new class {
    public int $value;
};
try {
    $ref =& $item->value;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
var_dump(isset($item->value));
--EXPECT--
Cannot access uninitialized non-nullable property class@anonymous::$value by reference
bool(false)
