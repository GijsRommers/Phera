--TEST--
Class conformance: contract-parameter-removed
--FILE--
<?php class P {function test($value){}} class C extends P {function test(){}}
--EXPECTF--

Fatal error: Declaration of C::test() must be compatible with P::test($value) in %s on line 1
