--TEST--
traits: static properties per reusing child
--FILE--
<?php
trait T{public static int $v=0;} class A{use T;} class B extends A{use T;} A::$v++; echo B::$v;
--EXPECT--
0
