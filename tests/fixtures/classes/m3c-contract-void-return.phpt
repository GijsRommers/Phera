--TEST--
Class conformance: contract-void-return
--FILE--
<?php class P {function test(): void{}} class C extends P {function test(): int{return 1;}}
--EXPECTF--

Fatal error: Declaration of C::test(): int must be compatible with P::test(): void in %s on line 1
