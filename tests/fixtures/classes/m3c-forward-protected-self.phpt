--TEST--
Class conformance: forward-protected-self
--FILE--
<?php class C {protected function hi(){return 6;} function go(){return self::hi();}}echo (new C)->go();
--EXPECT--
6
