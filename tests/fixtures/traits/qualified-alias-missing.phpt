--TEST--
traits: qualified alias missing
--FILE--
<?php
trait T{} class C{use T {T::missing as n;}}
--EXPECTF--
Fatal error: An alias was defined for T::missing but this method does not exist in %s on line 2
