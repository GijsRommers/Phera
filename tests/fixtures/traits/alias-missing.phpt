--TEST--
traits: alias missing
--FILE--
<?php
trait T{} class C{use T {missing as n;}}
--EXPECTF--
Fatal error: An alias (n) was defined for method missing(), but this method does not exist in /private%s on line 2
