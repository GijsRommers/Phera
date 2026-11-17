--TEST--
first class function
--FILE--
<?php
function f($x){return $x*2;}$g=f(...);var_dump($g instanceof Closure,$g(4));
--EXPECT--
bool(true)
int(8)
