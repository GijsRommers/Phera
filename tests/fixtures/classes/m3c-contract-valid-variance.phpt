--TEST--
Class conformance: contract-valid-variance
--FILE--
<?php class A {} class B extends A {} class P {function test(B $value): A{return $value;}} class C extends P {function test(A $value): B{return new B;}}echo "ok";
--EXPECT--
ok
