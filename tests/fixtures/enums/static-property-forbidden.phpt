--TEST--
enums: static property forbidden
--FILE--
<?php
enum E{public static int $x; case A;}
--EXPECTF--
Fatal error: Enum E cannot include properties in /private%s on line 2
