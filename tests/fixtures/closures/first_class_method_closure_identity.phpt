--TEST--
first class method closure identity
--FILE--
<?php
class A{function f(){return 3;}}$f=(new A)->f(...);$g=$f(...);var_dump($f===$g,$g());
--EXPECT--
bool(true)
int(3)
