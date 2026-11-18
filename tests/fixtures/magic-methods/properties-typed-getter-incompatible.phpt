--TEST--
An unset typed property names the getter when rejecting an incompatible value
--FILE--
<?php
declare(strict_types=1);
class Item {
    public string $label = '';
    function __construct() { unset($this->label); }
    function __get($name) { return 42; }
}
try { var_dump((new Item)->label); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Value of type int returned from Item::__get() must be compatible with unset property Item::$label of type string
