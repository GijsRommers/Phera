--TEST--
Class conformance: readonly-unset-scope
--FILE--
<?php class A {public readonly int $value;function clear(){unset($this->value);echo "ok";}}(new A)->clear();
--EXPECT--
ok
