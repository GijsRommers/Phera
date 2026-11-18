--TEST--
enums: enum case constant conflict with trait
--FILE--
<?php
trait T{const A=1;} enum E{use T;case A;}
--EXPECTF--
Fatal error: Cannot use trait T, because T::A conflicts with enum case E::A in %s on line 2
