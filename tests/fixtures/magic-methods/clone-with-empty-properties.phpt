--TEST--
clone with empty properties
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$copy = clone($original, []); echo $copy->value;
--EXPECT--
2
