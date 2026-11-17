--TEST--
Call window: float ten levels
--FILE--
<?php
function f(float $x,int $n):float{return $n?f($x,$n-1):$x+0.5;}var_dump(f(2,10));
--EXPECT--
float(2.5)
