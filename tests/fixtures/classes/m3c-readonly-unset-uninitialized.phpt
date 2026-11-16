--TEST--
Class conformance: readonly-unset-uninitialized
--FILE--
<?php class A {public readonly int $value;} $a=new A;unset($a->value);
--EXPECTF--

Fatal error: Uncaught Error: Cannot unset protected(set) readonly property A::$value from global scope in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
