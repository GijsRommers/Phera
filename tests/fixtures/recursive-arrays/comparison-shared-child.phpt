--TEST--
A nested array compares equal to the same array handle
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1, 5 => $a];
var_dump($a == $b, $a === $b);
--EXPECT--
bool(true)
bool(true)
