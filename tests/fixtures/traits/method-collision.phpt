--TEST--
traits: method collision
--FILE--
<?php
trait T{function m(){}} trait U{function m(){}} class C{use T,U;}
--EXPECTF--
Fatal error: Trait method U::m has not been applied as C::m, because of collision with T::m in %s on line 2
