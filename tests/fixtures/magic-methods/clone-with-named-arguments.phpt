--TEST--
clone with named arguments
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$copy = clone(object: $original, withProperties: ["value" => 10]); echo $copy->value;
--EXPECT--
10
