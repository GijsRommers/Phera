--TEST--
arrow nested shadow
--FILE--
<?php
$x=8;$f=fn($x)=>fn($y)=>$x+$y;$g=$f(2);var_dump($g(3));
--EXPECT--
int(5)
