--TEST--
clone with invalid type
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
try { clone($original, 2); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
clone(): Argument #2 ($withProperties) must be of type array, int given
