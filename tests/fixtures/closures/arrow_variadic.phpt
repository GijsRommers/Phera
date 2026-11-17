--TEST--
arrow variadic
--FILE--
<?php
$x=3;$f=fn(...$a)=>array_sum($a)+$x;var_dump($f(1,2,3));
--EXPECT--
int(9)
