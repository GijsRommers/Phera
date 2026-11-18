--TEST--
An earlier difference decides the result before a recursive entry
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
$b[0] = 2;
var_dump($a == $b, $a === $b);
--EXPECT--
bool(false)
bool(false)
