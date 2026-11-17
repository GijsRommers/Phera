--TEST--
arrow reference parameter
--FILE--
<?php
$f=fn(&$x)=>++$x;$x=2;var_dump($f($x),$x);
--EXPECT--
int(3)
int(3)
