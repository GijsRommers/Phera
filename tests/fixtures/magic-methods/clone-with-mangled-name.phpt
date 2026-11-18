--TEST--
clone with mangled name
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
try { clone($original, ["\0Copy\0value" => 3]); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot access property starting with "\0"
