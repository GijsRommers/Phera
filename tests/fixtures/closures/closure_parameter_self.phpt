--TEST--
closure parameter self
--FILE--
<?php
class A{function create(){return function(self $a){return $a instanceof self;};}}$a=new A;$f=$a->create();var_dump($f($a));
--EXPECT--
bool(true)
