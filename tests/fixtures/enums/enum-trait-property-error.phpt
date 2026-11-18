--TEST--
enums: enum trait property error
--FILE--
<?php
trait T{public $x;} enum E{use T;case A;}
--EXPECTF--
Fatal error: Enum E cannot include properties in /private%s on line 2
