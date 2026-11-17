--TEST--
closure named arguments
--FILE--
<?php
$x=4;$f=function($a,$b=2)use($x){return $x+$a*$b;};var_dump($f(b:3,a:5));
--EXPECT--
int(19)
