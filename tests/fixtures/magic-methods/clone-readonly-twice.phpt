--TEST--
A second readonly write in the hook fails
--FILE--
<?php
class Copy {
    public readonly int $value;
    function __construct() { $this->value = 1; }
    function __clone() { $this->value = 2; $this->value = 3; }
}
try { clone new Copy; } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot modify readonly property Copy::$value
