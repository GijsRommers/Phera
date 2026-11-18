--TEST--
enums: cannot declare from method
--FILE--
<?php
enum E: int{static function from($value){}}
--EXPECTF--
Fatal error: Cannot redeclare E::from() in %s on line 2
