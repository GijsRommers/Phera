--TEST--
Class members: static-redeclared
--FILE--
<?php class A {public static $x=1;} class B extends A {public static $x=5;} B::$x++; var_dump(A::$x,B::$x);
--EXPECT--
int(1)
int(6)
