--TEST--
Class conformance: readonly-other-scope
--FILE--
<?php class A {public readonly int $value;}class B {function go($a){$a->value=1;}} (new B)->go(new A);
--EXPECTF--

Fatal error: Uncaught Error: Cannot modify protected(set) readonly property A::$value from scope B in %s:1
Stack trace:
#0 %s(1): B->go(Object(A))
#1 {main}
  thrown in %s on line 1
