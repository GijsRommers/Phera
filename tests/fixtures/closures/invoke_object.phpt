--TEST--
invoke object
--FILE--
<?php
class Add{function __invoke($a,$b){return $a+$b;}}$f=new Add;var_dump($f(2,3),is_callable($f));
--EXPECT--
int(5)
bool(true)
