--TEST--
Class conformance: contract-required-extra
--FILE--
<?php class P {function test($value){}} class C extends P {function test($value,$extra){}}
--EXPECTF--

Fatal error: Declaration of C::test($value, $extra) must be compatible with P::test($value) in %s on line 1
