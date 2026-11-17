--TEST--
capture reference value
--FILE--
<?php
$a=3;$b=&$a;$f=function()use($b){return $b;};$a=7;var_dump($f());
--EXPECT--
int(3)
