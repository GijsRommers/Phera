--TEST--
variadic parent types
--FILE--
<?php
class A{}class B extends A{function f(){return fn(?parent ...$values)=>count($values);}}
$f=(new B)->f();var_dump($f(new A,null,new B));
--EXPECT--
int(3)
