--TEST--
enums: property forbidden
--FILE--
<?php
enum E{public int $x; case A;}
--EXPECTF--
Fatal error: Enum E cannot include properties in %s on line 2
