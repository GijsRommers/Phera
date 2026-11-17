--TEST--
Class conformance: contract-interface
--FILE--
<?php interface P {function test(int $value): int;} class C implements P {function test(string $value): int{return 1;}}
--EXPECTF--

Fatal error: Declaration of C::test(string $value): int must be compatible with P::test(int $value): int in %s on line 1
