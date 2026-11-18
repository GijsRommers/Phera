--TEST--
clone with reference
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$value = 4; try { clone($original, ["value" => &$value]); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot assign by reference when cloning with updated properties
