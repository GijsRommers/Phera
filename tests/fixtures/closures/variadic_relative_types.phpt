--TEST--
variadic relative types
--FILE--
<?php
class A{function f(){return function(self ...$values){return count($values);};}}
class B{}$f=(new A)->f();var_dump($f(new A,new A));$g=$f->bindTo(new B,B::class);var_dump($g(new B));
--EXPECT--
int(2)
int(1)
