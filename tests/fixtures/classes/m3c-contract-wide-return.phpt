--TEST--
Class conformance: contract-wide-return
--FILE--
<?php class A {} class B extends A {} class P {function test(): B{return new B;}} class C extends P {function test(): A{return new A;}}
--EXPECTF--

Fatal error: Declaration of C::test(): A must be compatible with P::test(): B in %s on line 1
