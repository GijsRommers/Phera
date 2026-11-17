--TEST--
Class conformance: forward-public-static
--FILE--
<?php class C {public function hi(){return 6;} function go(){return static::hi();}}echo (new C)->go();
--EXPECT--
6
