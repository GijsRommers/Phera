--TEST--
Class conformance: probe-subclass-function
--FILE--
<?php class P {public $v=5;}class C extends P {}function f(P $p): P{return $p;}echo f(new C)->v;
--EXPECT--
5
