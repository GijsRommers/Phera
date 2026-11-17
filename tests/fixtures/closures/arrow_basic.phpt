--TEST--
arrow basic
--FILE--
<?php
$x=4;$f=fn($y)=>$x+$y;$x=10;var_dump($f(3));
--EXPECT--
int(7)
