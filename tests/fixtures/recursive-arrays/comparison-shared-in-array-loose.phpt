--TEST--
in-array-loose compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(in_array($a, [$b]));
--EXPECT--
bool(true)
