--TEST--
traits: final adaptation forbids override
--FILE--
<?php
trait T { function m() {} } class C { use T { m as final; } } class D extends C { function m() {} }
--EXPECTF--
Fatal error: Cannot override final method C::m() in %s on line 2
