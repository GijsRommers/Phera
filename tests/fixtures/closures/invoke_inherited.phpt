--TEST--
invoke inherited
--FILE--
<?php
class A{protected $x=2;function __invoke($a){return $this->x+$a;}}class B extends A{}$f=new B;var_dump($f(3));
--EXPECT--
int(5)
