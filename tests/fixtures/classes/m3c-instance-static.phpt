--TEST--
Class conformance: instance-static
--FILE--
<?php class C {function test(){return $this instanceof static;}} var_dump((new C)->test());
--EXPECT--
bool(true)
