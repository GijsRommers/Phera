--TEST--
parameters types
--FILE--
<?php
$f=function(int $x):int{return $x+1;};var_dump($f("4"));
--EXPECT--
int(5)
