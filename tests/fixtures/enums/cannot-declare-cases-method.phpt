--TEST--
enums: cannot declare cases method
--FILE--
<?php
enum E{static function cases(){}}
--EXPECTF--
Fatal error: Cannot redeclare E::cases() in /private%s on line 2
