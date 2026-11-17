--TEST--
static closure property
--FILE--
<?php
class A{public $callback=static function($x){return $x*2;};}$a=new A;$f=$a->callback;var_dump($f(3));
--EXPECT--
int(6)
