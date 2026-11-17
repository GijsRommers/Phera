--TEST--
closure this reference
--FILE--
<?php
class A{function f(){$a=&$this;return function()use(&$a){return $a;};}}$a=new A;$f=$a->f();var_dump($f()===$a);
--EXPECT--
bool(true)
