--TEST--
Class conformance: forward-private-self
--FILE--
<?php class C {private function hi(){return 6;} function go(){return self::hi();}}echo (new C)->go();
--EXPECT--
6
