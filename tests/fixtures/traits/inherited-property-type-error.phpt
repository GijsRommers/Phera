--TEST--
traits: inherited property type error
--FILE--
<?php
trait T{public int $a;} class P{public float $a;} class C extends P{use T;}
--EXPECTF--
Fatal error: Type of C::$a must be float (as in class P) in /private%s on line 2
