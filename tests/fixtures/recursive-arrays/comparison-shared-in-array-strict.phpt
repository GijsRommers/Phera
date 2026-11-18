--TEST--
in-array-strict compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(in_array($a, [$b], true));
--EXPECT--
bool(true)
