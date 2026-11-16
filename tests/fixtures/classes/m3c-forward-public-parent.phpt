--TEST--
Class conformance: forward-public-parent
--FILE--
<?php class P {public function hi(){return 6;}}class C extends P {function go(){return parent::hi();}}echo (new C)->go();
--EXPECT--
6
