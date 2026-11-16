--TEST--
Class conformance: contract-optional-required
--FILE--
<?php class P {function test($value=null){}} class C extends P {function test($value){}}
--EXPECTF--

Fatal error: Declaration of C::test($value) must be compatible with P::test($value = null) in %s on line 1
