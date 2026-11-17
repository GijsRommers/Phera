--TEST--
closure fastpath depth guard
--FILE--
<?php
class A{}$n=PHP_INT_MAX;$f=fn($x)=>$x+$n;var_dump($f(1));
--EXPECT--
float(9.223372036854776E+18)
