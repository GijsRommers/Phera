--TEST--
traits: inherited property visibility error
--FILE--
<?php
trait T{protected $x=5;} class P{public $x=4;} class C extends P{use T;}
--EXPECTF--
Fatal error: Access level to C::$x must be public (as in class P) in /private%s on line 2
