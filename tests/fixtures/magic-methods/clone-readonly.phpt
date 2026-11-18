--TEST--
The clone hook may replace a readonly property once
--FILE--
<?php
class Copy {
    public readonly int $value;
    function __construct() { $this->value = 1; }
    function __clone() { $this->value = 2; }
}
$copy = clone new Copy;
echo $copy->value;
--EXPECT--
2
