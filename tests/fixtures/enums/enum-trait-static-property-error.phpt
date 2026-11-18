--TEST--
enums: enum trait static property error
--FILE--
<?php
trait T{public static $x;} enum E{use T;}
--EXPECTF--
Fatal error: Enum E cannot include properties in /private%s on line 2
