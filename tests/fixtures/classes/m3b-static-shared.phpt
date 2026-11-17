--TEST--
Class members: static-shared
--FILE--
<?php class A {public static $x=1;} class B extends A {} B::$x++; var_dump(A::$x);
--EXPECT--
int(2)
