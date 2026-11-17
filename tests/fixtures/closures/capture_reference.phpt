--TEST--
capture reference
--FILE--
<?php
$a=1;$f=function()use(&$a){return ++$a;};var_dump($f(),$f(),$a);
--EXPECT--
int(2)
int(3)
int(3)
