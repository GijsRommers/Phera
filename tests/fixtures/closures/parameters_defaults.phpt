--TEST--
parameters defaults
--FILE--
<?php
$f=function($a=2,$b=3){return $a+$b;};var_dump($f(),$f(8),$f(b:9));
--EXPECT--
int(5)
int(11)
int(11)
