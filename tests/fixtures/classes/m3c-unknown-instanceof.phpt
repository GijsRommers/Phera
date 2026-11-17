--TEST--
Class conformance: unknown-instanceof
--FILE--
<?php class C {} var_dump((new C) instanceof Missing);
--EXPECT--
bool(false)
