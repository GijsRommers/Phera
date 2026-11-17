--TEST--
Class conformance: contract-reference-param
--FILE--
<?php class P {function test(&$value){}} class C extends P {function test($value){}}
--EXPECTF--

Fatal error: Declaration of C::test($value) must be compatible with P::test(&$value) in %s on line 1
