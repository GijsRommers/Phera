--TEST--
Visiting the same array in two branches is allowed
--FILE--
<?php
$x = [1];
$a = [$x, $x];
$b = [[1], [1]];
var_dump($a == $b, $a === $b);
--EXPECT--
bool(true)
bool(true)
