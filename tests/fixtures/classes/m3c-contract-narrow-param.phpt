--TEST--
Class conformance: contract-narrow-param
--FILE--
<?php class A {} class B extends A {} class P {function test(A $value){}} class C extends P {function test(B $value){}}
--EXPECTF--

Fatal error: Declaration of C::test(B $value) must be compatible with P::test(A $value) in %s on line 1
