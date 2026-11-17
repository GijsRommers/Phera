--TEST--
Class conformance: static-as-instance
--FILE--
<?php class A {public static $value=5;} $a=new A;var_dump($a->value);$a->value=8;var_dump($a->value,A::$value);
--EXPECTF--

Notice: Accessing static property A::$value as non static in %s on line 1

Warning: Undefined property: A::$value in %s on line 1
NULL

Notice: Accessing static property A::$value as non static in %s on line 1

Deprecated: Creation of dynamic property A::$value is deprecated in %s on line 1

Notice: Accessing static property A::$value as non static in %s on line 1
int(8)
int(5)
