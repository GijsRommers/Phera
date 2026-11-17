--TEST--
closure spread
--FILE--
<?php
$x=3;$f=function(...$args)use($x){return array_sum($args)+$x;};var_dump($f(...[1,2,3]));
--EXPECT--
int(9)
