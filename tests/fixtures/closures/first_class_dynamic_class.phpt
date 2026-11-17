--TEST--
first class dynamic class
--FILE--
<?php
class A{static function f($x){return $x+2;}}$class="A";$name="f";$g=$class::$name(...);var_dump($g(3));
--EXPECT--
int(5)
