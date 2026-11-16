--TEST--
Class conformance: contract-valid-nullable
--FILE--
<?php class A {} class P {function test(A $value): ?A{return $value;}} class C extends P {function test(?A $value): A{return new A;}}echo "ok";
--EXPECT--
ok
