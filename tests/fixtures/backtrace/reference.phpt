--TEST--
Trace arguments copy the value of a parameter reference
--FILE--
<?php
function inspect(&$a) {
    $trace = debug_backtrace()[0]['args'];
    $a = 42;
    var_export($trace);
}
$value = 10;
inspect($value);
--EXPECT--
array (
  0 => 10,
)
