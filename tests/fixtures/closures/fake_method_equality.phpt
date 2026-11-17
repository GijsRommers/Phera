--TEST--
fake method equality
--FILE--
<?php
class A{function m(){}static function s(){}}class B extends A{}
$a=new A;$b=new A;$f=$a->m(...);var_dump($f==$a->m(...),$f==$b->m(...),A::s(...)==B::s(...),$f==clone $f);
--EXPECT--
bool(true)
bool(false)
bool(false)
bool(true)
