--TEST--
closure this parameter
--FILE--
<?php
function same(&$value){return $value;}class A{function f(){return same($this);}}$a=new A;var_dump($a->f()===$a);
--EXPECT--
bool(true)
