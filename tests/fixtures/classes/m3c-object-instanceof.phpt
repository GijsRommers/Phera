--TEST--
Class conformance: object-instanceof
--FILE--
<?php class C {} $o=new C; var_dump($o instanceof $o);
--EXPECT--
bool(true)
