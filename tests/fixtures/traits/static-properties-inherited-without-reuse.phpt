--TEST--
traits: static properties inherited without reuse
--FILE--
<?php
trait T{public static int $v=0;} class A{use T;} class B extends A{} A::$v++; echo B::$v;
--EXPECT--
1
