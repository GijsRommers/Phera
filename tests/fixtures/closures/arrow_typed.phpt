--TEST--
arrow typed
--FILE--
<?php
$f=fn(int $x):string=>(string)($x*2);var_dump($f(4));
--EXPECT--
string(1) "8"
