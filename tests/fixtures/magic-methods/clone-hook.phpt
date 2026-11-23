--TEST--
clone hook
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$copy = clone $original; echo $copy->value, "/", $original->value;
--EXPECT--
2/1
