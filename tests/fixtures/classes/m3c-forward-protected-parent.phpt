--TEST--
Class conformance: forward-protected-parent
--FILE--
<?php class P {protected function hi(){return 6;}}class C extends P {function go(){return parent::hi();}}echo (new C)->go();
--EXPECT--
6
