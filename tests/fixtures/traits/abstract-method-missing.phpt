--TEST--
traits: abstract method missing
--FILE--
<?php
trait T{abstract function m();} class C{use T;}
--EXPECTF--
Fatal error: Class C contains 1 abstract method and must therefore be declared abstract or implement the remaining method (C::m) in %s on line 2
