--TEST--
first class private method
--FILE--
<?php
class A{private function f($x){return $x+2;}function get(){return $this->f(...);}}$g=(new A)->get();var_dump($g(3));
--EXPECT--
int(5)
