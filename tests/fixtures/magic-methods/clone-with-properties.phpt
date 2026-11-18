--TEST--
clone with properties
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$copy = clone($original, ["value"=>10]); echo $copy->value;
--EXPECT--
10
