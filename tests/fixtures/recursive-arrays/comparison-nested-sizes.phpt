--TEST--
Loose comparison checks the size of nested arrays
--FILE--
<?php
$a = [[1, 2]];
$b = [[1]];
var_dump($a == $b, $a <=> $b);
--EXPECT--
bool(false)
int(1)
