--TEST--
traits: static properties per using class
--FILE--
<?php
trait T{public static int $v=0;} class A{use T;} class B{use T;} A::$v++; echo B::$v;
--EXPECT--
0
