--TEST--
min compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(is_array(min($a, $b)));
--EXPECT--
bool(true)
