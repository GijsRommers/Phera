--TEST--
Class conformance: readonly-unset-initialized
--FILE--
<?php class A {public readonly int $value;function __construct(){$this->value=1;}}$a=new A;unset($a->value);
--EXPECTF--

Fatal error: Uncaught Error: Cannot unset readonly property A::$value in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
