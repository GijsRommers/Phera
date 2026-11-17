--TEST--
callback private scope
--FILE--
<?php
class A{private function f(){return 7;}function test(){return is_callable([$this,"f"]);}}var_dump((new A)->test());
--EXPECT--
bool(true)
