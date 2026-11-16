--TEST--
Class conformance: forward-public-self
--FILE--
<?php class C {public function hi(){return 6;} function go(){return self::hi();}}echo (new C)->go();
--EXPECT--
6
