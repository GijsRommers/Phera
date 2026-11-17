--TEST--
closure rebind parameter self
--FILE--
<?php
class A{}class B{}$f=function(self $value){return $value instanceof self;};$g=Closure::bind($f,null,B::class);var_dump($g(new B));
--EXPECT--
bool(true)
