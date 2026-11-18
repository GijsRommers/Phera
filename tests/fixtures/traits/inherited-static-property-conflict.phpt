--TEST--
traits: inherited static property conflict
--FILE--
<?php
trait T{public $a="c";} class P{public static $a="p";} class C extends P{use T;}
--EXPECTF--
Fatal error: Cannot redeclare static P::$a as non static C::$a in /private%s on line 2
