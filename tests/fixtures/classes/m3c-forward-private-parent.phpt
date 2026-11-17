--TEST--
Class conformance: forward-private-parent
--FILE--
<?php class P {private function hi(){return 6;}}class C extends P {function go(){return parent::hi();}}echo (new C)->go();
--EXPECTF--

Fatal error: Uncaught Error: Call to private method P::hi() from scope C in %s:1
Stack trace:
#0 %s(1): C->go()
#1 {main}
  thrown in %s on line 1
