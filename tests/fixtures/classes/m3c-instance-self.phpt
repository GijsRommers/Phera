--TEST--
Class conformance: instance-self
--FILE--
<?php class C {function test(){return $this instanceof self;}} var_dump((new C)->test());
--EXPECT--
bool(true)
