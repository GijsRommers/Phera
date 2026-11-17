--TEST--
invoke method
--FILE--
<?php
$x=3;$f=function($a)use($x){return $x+$a;};var_dump($f->__invoke(4));
--EXPECT--
int(7)
