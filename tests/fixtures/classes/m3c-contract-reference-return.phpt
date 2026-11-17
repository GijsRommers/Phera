--TEST--
Class conformance: contract-reference-return
--FILE--
<?php class P {function &test(){}} class C extends P {function test(){}}
--EXPECTF--

Fatal error: Declaration of C::test() must be compatible with & P::test() in %s on line 1
