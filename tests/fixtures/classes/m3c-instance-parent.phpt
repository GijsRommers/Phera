--TEST--
Class conformance: instance-parent
--FILE--
<?php class P {} class C extends P {function test(){return $this instanceof parent;}} var_dump((new C)->test());
--EXPECT--
bool(true)
