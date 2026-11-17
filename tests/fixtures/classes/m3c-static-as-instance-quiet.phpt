--TEST--
Class conformance: static-as-instance-quiet
--FILE--
<?php class A {public static $value=5;} $a=new A;var_dump(isset($a->value));unset($a->value);
--EXPECTF--
bool(false)

Notice: Accessing static property A::$value as non static in %s on line 1
