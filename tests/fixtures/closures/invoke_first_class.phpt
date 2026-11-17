--TEST--
invoke first class
--FILE--
<?php
class Add{function __invoke($a){return $a+4;}}$a=new Add;$f=$a(...);var_dump($f instanceof Closure,$f(3));
--EXPECT--
bool(true)
int(7)
