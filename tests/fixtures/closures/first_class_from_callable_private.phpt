--TEST--
first class from callable private
--FILE--
<?php
class A{private function f(){return 8;}function get(){return Closure::fromCallable([$this,"f"]);}}$g=(new A)->get();var_dump($g());
--EXPECT--
int(8)
