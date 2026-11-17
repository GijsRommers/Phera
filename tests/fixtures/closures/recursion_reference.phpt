--TEST--
recursion reference
--FILE--
<?php
$f=function($n)use(&$f){return $n<2?1:$n*$f($n-1);};var_dump($f(8));
--EXPECT--
int(40320)
