--TEST--
Class conformance: contract-valid-union
--FILE--
<?php class A {} class P {function test(A $value): A|int{return $value;}} class C extends P {function test(A|int $value): A{return new A;}}echo "ok";
--EXPECT--
ok
