--TEST--
arrow defaults
--FILE--
<?php
$x=3;$f=fn($a=2)=>$a+$x;var_dump($f(),$f(8));
--EXPECT--
int(5)
int(11)
