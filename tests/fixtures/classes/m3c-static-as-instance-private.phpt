--TEST--
Class conformance: static-as-instance-private
--FILE--
<?php class A {private static $value=5;} $a=new A;var_dump($a->value);
--EXPECTF--

Fatal error: Uncaught Error: Cannot access private property A::$value in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
