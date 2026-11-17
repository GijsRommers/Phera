--TEST--
capture value
--FILE--
<?php
$a=1;$f=function()use($a){return $a;};$a=2;var_dump($f(),$a);
--EXPECT--
int(1)
int(2)
