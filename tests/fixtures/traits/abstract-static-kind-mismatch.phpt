--TEST--
traits: abstract static kind mismatch
--FILE--
<?php
trait T{abstract static function m();} class C{use T;function m(){}}
--EXPECTF--
Fatal error: Cannot make static method T::m() non static in class C in %s on line 2
