--TEST--
Class conformance: forward-protected-static
--FILE--
<?php class C {protected function hi(){return 6;} function go(){return static::hi();}}echo (new C)->go();
--EXPECT--
6
