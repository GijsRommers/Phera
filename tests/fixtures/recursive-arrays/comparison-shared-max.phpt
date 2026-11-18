--TEST--
max compares shared recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = $a;
var_dump(is_array(max([$a, $b])));
--EXPECT--
bool(true)
