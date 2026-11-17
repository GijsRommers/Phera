--TEST--
first class dynamic method
--FILE--
<?php
class A{function f($x){return $x+2;}}$o=new A;$name="f";$g=$o->$name(...);var_dump($g(3));
--EXPECT--
int(5)
