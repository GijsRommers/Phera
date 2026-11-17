--TEST--
resolve private error
--FILE--
<?php
class A{private function f(){}}$f=[new A,"f"];try{$f();}catch(Error $e){echo $e->getMessage(),"
";}var_dump(is_callable($f));
--EXPECT--
Call to private method A::f() from global scope
bool(false)
