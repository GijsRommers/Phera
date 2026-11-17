--TEST--
Class conformance: forward-private-static
--FILE--
<?php class C {private function hi(){return 6;} function go(){return static::hi();}}echo (new C)->go();
--EXPECT--
6
