--TEST--
invoke from callable
--FILE--
<?php
class Add{private $x=4;function __invoke($a){return $a+$this->x;}}$f=Closure::fromCallable(new Add);var_dump($f instanceof Closure,$f(3));
--EXPECT--
bool(true)
int(7)
