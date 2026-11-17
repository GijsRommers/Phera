--TEST--
parameters reference
--FILE--
<?php
$f=function(&$x){$x*=2;};$a=3;$f($a);var_dump($a);
--EXPECT--
int(6)
