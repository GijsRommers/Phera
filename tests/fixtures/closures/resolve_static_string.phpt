--TEST--
resolve static string
--FILE--
<?php
class A{static function f($x){return $x+1;}}$f="A::f";var_dump($f(2),is_callable($f));
--EXPECT--
int(3)
bool(true)
